//! Generates the private-access values to put in Heroku → Settings → Config Vars (checked by `altim::auth`):
//!   cargo run --release --bin altim-secrets                         random password (shown once, only its hash goes to Heroku)
//!   cargo run --release --bin altim-secrets -- --user max           choose the login name
//!   cargo run --release --bin altim-secrets -- --password "…"       hash a password of your choice (16 characters minimum)
//! Nothing is written to disk: copy the values, keep the password in a password manager.
use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version};
use base64::Engine;

/// RFC 4648 base32 without padding (the 2FA secret, as authenticator apps expect it).
fn base32(bytes: &[u8]) -> String {
    const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let bits: String = bytes.iter().map(|b| format!("{b:08b}")).collect();
    bits.as_bytes()
        .chunks(5)
        .map(|c| {
            let chunk = format!("{:0<5}", std::str::from_utf8(c).unwrap_or("0"));
            B32[usize::from_str_radix(&chunk, 2).unwrap_or(0)] as char
        })
        .collect()
}

/// 24 characters from a 56-symbol alphabet without look-alikes (≈ 139 bits of entropy), in groups of 6.
fn password() -> String {
    // The first 56 symbols are drawn (the final "9" never is, like the former Bun script).
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut out = String::new();
    while out.len() < 24 {
        let bytes: [u8; 32] = rand::random();
        // 224 = 4 × 56: no modulo bias.
        for b in bytes.into_iter().filter(|b| *b < 224) {
            if out.len() < 24 {
                out.push(ALPHABET[usize::from(b) % 56] as char);
            }
        }
    }
    out.as_bytes().chunks(6).map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join("-")
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == &format!("--{name}")).and_then(|i| args.get(i + 1)).cloned()
}

/// `encodeURIComponent`.
fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let user = arg(&args, "user").unwrap_or_else(|| "max".into());
    let chosen = arg(&args, "password");
    if chosen.as_ref().is_some_and(|p| p.chars().count() < 16) {
        eprintln!("Mot de passe trop court : 16 caractères minimum.");
        std::process::exit(1);
    }
    let pass = chosen.unwrap_or_else(password);
    // argon2id, 64 MiB, 3 passes, 1 lane (the parameters of the former `Bun.password.hash`).
    let params = Params::new(65536, 3, 1, None).expect("paramètres argon2");
    let hash = Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password(pass.as_bytes()).expect("hachage argon2").to_string();
    let totp = base32(&rand::random::<[u8; 20]>()); // 160 bits, RFC 4226 recommendation
    let session = hex::encode(rand::random::<[u8; 64]>());
    let api = format!("altim_{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>()));
    let uri = format!("otpauth://totp/Altim:{}?secret={totp}&issuer=Altim&algorithm=SHA1&digits=6&period=30", encode(&user));
    println!(
        "
À mettre dans Heroku → Settings → Config Vars :

  ALTIM_USER            = {user}
  ALTIM_PASSWORD_HASH   = {hash}
  ALTIM_TOTP_SECRET     = {totp}
  ALTIM_SESSION_SECRET  = {session}
  ALTIM_API_TOKEN       = {api}

À garder pour vous (gestionnaire de mots de passe), jamais dans Heroku ni dans le code :

  Mot de passe          : {pass}
  Application 2FA       : ajoutez le secret {totp} (ou ce lien : {uri})
  Jeton pour vos bots   : Authorization: Bearer {api}
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        // RFC 4648 test vectors (without padding).
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32(&[0u8; 20]).len(), 32);
        let p = password();
        assert_eq!(p.len(), 24 + 3);
        assert!(p.split('-').all(|g| g.len() == 6 && g.bytes().all(|b| b.is_ascii_alphanumeric() && !b"0O1lI".contains(&b))));
        assert_eq!(encode("max lestage"), "max%20lestage");
        // The server accepts what is generated here.
        let params = Params::new(8192, 1, 1, None).unwrap();
        let hash = Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password(b"un mot de passe assez long").unwrap().to_string();
        assert!(hash.starts_with("$argon2id$v=19$m=8192,t=1,p=1$"));
        assert!(altim::auth::verify_password("un mot de passe assez long", &hash));
        assert!(!altim::auth::verify_password("autre", &hash));
    }
}
