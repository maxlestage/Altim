package com.maxlestage.altim.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Encrypted storage for the credentials and the session cookie: AES-256-GCM with a key kept in the Android Keystore
 * (hardware-backed when the phone has it, never exportable). The key is created on this phone and is not part of
 * any backup (backups are disabled anyway), so the values cannot be read elsewhere. Equivalent of the iOS Keychain
 * "ThisDeviceOnly".
 */
class SecureStore(context: Context) : SecretStore {
    private val prefs = context.getSharedPreferences("altim.secure", Context.MODE_PRIVATE)

    /**
     * The password's key only works while the phone is unlocked (a stolen, locked phone cannot decrypt it). The
     * session's key also works while it is locked: the background check of the buy alerts needs it.
     */
    enum class Key(val raw: String, val alias: String, val unlockedOnly: Boolean) {
        CREDENTIALS("credentials", "altim.access.credentials", true),
        SESSION("session", "altim.access.session", false),
    }

    private fun key(k: Key): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getEntry(k.alias, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(k.alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setUnlockedDeviceRequired(k.unlockedOnly)
                .build(),
        )
        return gen.generateKey()
    }

    override fun set(k: Key, value: String?) {
        if (value.isNullOrEmpty()) return prefs.edit().remove(k.raw).apply()
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key(k)) }
        val sealed = cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        prefs.edit().putString(k.raw, Base64.encodeToString(sealed, Base64.NO_WRAP)).apply()
    }

    override fun get(k: Key): String? {
        val raw = prefs.getString(k.raw, null) ?: return null
        return runCatching {
            val sealed = Base64.decode(raw, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(k), GCMParameterSpec(128, sealed, 0, IV_BYTES))
            cipher.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES).toString(Charsets.UTF_8)
        }.getOrNull()
    }

    override fun clear() = prefs.edit().clear().apply()

    private companion object {
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
    }
}

/** Storage of the secrets; the tests use an in-memory one (no Android Keystore on the JVM). */
interface SecretStore {
    fun set(k: SecureStore.Key, value: String?)
    fun get(k: SecureStore.Key): String?
    fun clear()
}
