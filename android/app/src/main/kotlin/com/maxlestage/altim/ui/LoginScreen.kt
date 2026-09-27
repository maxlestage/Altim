package com.maxlestage.altim.ui

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.data.AppModel
import com.maxlestage.altim.kit.AccessMode
import com.maxlestage.altim.kit.Tone
import kotlinx.coroutines.launch
import okhttp3.HttpUrl

/**
 * Connection to the private Altim server (the Heroku app): address, then user + password
 * (+ 6-digit code only if 2FA is on the server).
 */
@Composable
fun LoginScreen(model: AppModel) {
    val prefs = LocalContext.current.getSharedPreferences("altim.login", Context.MODE_PRIVATE)
    var server by remember { mutableStateOf(prefs.getString("serverInput", "") ?: "") }
    var user by remember { mutableStateOf(prefs.getString("lastUser", "") ?: "") }
    var password by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var mode by remember { mutableStateOf<AccessMode?>(null) }
    var url by remember { mutableStateOf<HttpUrl?>(null) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    val needsCode = (mode as? AccessMode.Login)?.needsCode == true
    val canSubmit = server.isNotBlank() && when (mode) {
        is AccessMode.Login -> user.isNotEmpty() && password.isNotEmpty() && (!needsCode || code.length == 6)
        else -> true
    }

    fun check() = scope.launch {
        busy = true
        error = null
        try {
            val (u, m) = model.probe(server)
            url = u
            mode = m
            prefs.edit().putString("serverInput", server).apply()
        } catch (e: Exception) {
            error = e.message
        } finally {
            busy = false
        }
    }

    fun connect() = scope.launch {
        val u = url
        val m = mode
        if (u == null || m == null) return@launch
        busy = true
        error = null
        try {
            model.connect(u, m, user, password, code)
            prefs.edit().putString("lastUser", user.trim()).apply()
            password = ""
            code = ""
        } catch (e: Exception) {
            error = e.message
        } finally {
            busy = false
        }
    }

    Column(
        Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        Text("ALTIM", style = TextStyle(brush = AltimColors.accentGradient, fontSize = 48.sp, fontWeight = FontWeight.Black), modifier = Modifier.padding(top = 32.dp))
        Text("Accès privé", fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Text(
            "Altim se connecte à votre serveur (l'application Heroku) : toutes les analyses y sont calculées sur 40 sources. Le mot de passe est gardé chiffré sur ce téléphone (clé du Keystore Android).",
            color = AltimColors.textSecondary, fontSize = 14.sp,
        )
        Card {
            Field("Adresse du serveur", server, "mon-app.herokuapp.com", KeyboardType.Uri, ImeAction.Next, onDone = { check() }) {
                server = it
                mode = null
            }
            if (mode is AccessMode.Login) {
                Field("Identifiant", user, "max", KeyboardType.Ascii, ImeAction.Next) { user = it }
                Field("Mot de passe", password, "••••••", KeyboardType.Password, if (needsCode) ImeAction.Next else ImeAction.Go, secret = true, onDone = { if (!needsCode && canSubmit) connect() }) { password = it }
                if (needsCode) {
                    Field("Code à 6 chiffres", code, "123456", KeyboardType.NumberPassword, ImeAction.Go, onDone = { if (canSubmit) connect() }) { code = it.filter(Char::isDigit).take(6) }
                    Caption("Double authentification active sur le serveur : le code sera redemandé quand la session expire (7 jours).")
                }
            }
            if (mode == AccessMode.Open) Notice("Serveur de développement sans accès privé : aucune connexion demandée.", Tone.WARN)
        }
        error?.let { Notice(it, Tone.BAD) }
        NeonButton(if (mode == null) "CONTINUER" else "SE CONNECTER", enabled = canSubmit, busy = busy) {
            if (mode == null) check() else connect()
        }
        Caption("Sans réseau vers le serveur, l'app ne peut rien afficher : aucune donnée de marché n'est inventée ni gardée en cache au-delà de quelques minutes.")
    }
}

@Composable
private fun Field(
    label: String,
    value: String,
    placeholder: String,
    type: KeyboardType,
    ime: ImeAction,
    secret: Boolean = false,
    onDone: () -> Unit = {},
    onChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        placeholder = { Text(placeholder, color = AltimColors.textSecondary) },
        singleLine = true,
        visualTransformation = if (secret) PasswordVisualTransformation() else androidx.compose.ui.text.input.VisualTransformation.None,
        keyboardOptions = KeyboardOptions(keyboardType = type, imeAction = ime, autoCorrectEnabled = false),
        keyboardActions = KeyboardActions(onAny = { onDone() }),
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = AltimColors.cyan, focusedLabelColor = AltimColors.cyan, cursorColor = AltimColors.cyan),
    )
}
