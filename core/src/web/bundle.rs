//! Which part of the web front serves an address. scripts/build-web.sh builds one .wasm for the presentation site and
//! one per group of app screens, so that a first visit downloads only the screens of its group; the server sends the
//! page of the address's group (`page`), and the front loads an address of another group as a full page. The same
//! function decides on both sides, from the raw path (`location.pathname` / the request path).
use crate::types::Kind;

/// The groups of app screens, each with its .wasm and its page `web/dist/app/<page>.html` (`index` for the Radar).
pub const APP_BUNDLES: [&str; 7] = ["radar", "actif", "avoirs", "selection", "actu", "reglages", "bot"];

/// `^(crypto|stock)/[A-Za-z0-9.-]{1,10}$` of WebApp.tsx: the asset of /app/actif/…, symbol in upper case.
pub fn asset_of(kind: &str, symbol: &str) -> Option<(Kind, String)> {
    let kind = Kind::parse(kind)?;
    let ok = (1..=10).contains(&symbol.len()) && symbol.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    ok.then(|| (kind, symbol.to_ascii_uppercase()))
}

/// "site" or one of `APP_BUNDLES` for a path (query and fragment ignored, trailing slashes too). Any other /app/…
/// address shows the Radar, like a malformed asset address.
pub fn bundle_of(path: &str) -> &'static str {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let p = path.trim_end_matches('/');
    if p != "/app" && !p.starts_with("/app/") {
        return "site";
    }
    match p {
        "/app/avoirs" | "/app/simulation" | "/app/journal" => "avoirs",
        "/app/selection" | "/app/opportunites" => "selection",
        "/app/actu" => "actu",
        "/app/reglages" | "/app/lexique" => "reglages",
        "/app/bot" | "/app/validation" => "bot",
        _ => match p.strip_prefix("/app/actif/").and_then(|r| r.split_once('/')) {
            Some((kind, symbol)) if asset_of(kind, symbol).is_some() => "actif",
            _ => "radar",
        },
    }
}

/// The page of an app group in web/dist/app (without `.html`).
pub fn page(bundle: &str) -> &str {
    if bundle == "radar" { "index" } else { bundle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups() {
        for (path, b) in [
            ("/", "site"),
            ("/risques", "site"),
            ("/application", "site"),
            ("/app", "radar"),
            ("/app/", "radar"),
            ("/app/inconnu", "radar"),
            ("/app/alertes", "radar"),
            ("/app/actif/crypto/BTC", "actif"),
            ("/app/actif/stock/brk.b/", "actif"),
            ("/app/actif/fx/EUR", "radar"),
            ("/app/actif/stock/TOOLONGSYMBOL", "radar"),
            ("/app/actif/stock/A%20B", "radar"),
            ("/app/actif/crypto/BTC/x", "radar"),
            ("/app/avoirs", "avoirs"),
            ("/app/journal?x=1", "avoirs"),
            ("/app/simulation#top", "avoirs"),
            ("/app/opportunites", "selection"),
            ("/app/actu", "actu"),
            ("/app/reglages", "reglages"),
            ("/app/bot", "bot"),
            ("/app/validation", "bot"),
            ("/app/lexique", "reglages"),
        ] {
            assert_eq!(bundle_of(path), b, "{path}");
        }
        assert_eq!(page("radar"), "index");
        assert_eq!(page("actif"), "actif");
        assert!(APP_BUNDLES.iter().all(|b| bundle_of(&format!("/app/{b}")) != "site"));
    }

    #[test]
    fn asset_paths() {
        assert_eq!(asset_of("crypto", "btc"), Some((Kind::Crypto, "BTC".into())));
        assert_eq!(asset_of("stock", "BRK.B"), Some((Kind::Stock, "BRK.B".into())));
        assert_eq!(asset_of("fx", "EUR"), None);
        assert_eq!(asset_of("stock", "TOOLONGSYMBOL"), None);
        assert_eq!(asset_of("stock", "A B"), None);
    }
}
