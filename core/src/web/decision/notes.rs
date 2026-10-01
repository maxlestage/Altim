//! Personal notes on an asset (NoteCard.tsx): "altim.notes.v1" = `{ "crypto:BTC": { text, updated } }`, kept in this
//! browser only, never sent to the server.
use serde_json::{Map, Value, json};

pub const NOTES_KEY: &str = "altim.notes.v1";
/// Longest note kept (characters, like `slice(0, 2000)` on UTF-16 units for the usual text).
pub const NOTE_MAX: usize = 2000;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub text: String,
    pub updated: Option<f64>,
}

fn load(raw: Option<&str>) -> Map<String, Value> {
    match raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// The note of an asset ("" when none).
pub fn note(raw: Option<&str>, id: &str) -> Note {
    let all = load(raw);
    let n = all.get(id);
    Note {
        text: n.and_then(|n| n.get("text")).and_then(Value::as_str).unwrap_or_default().to_string(),
        updated: n.and_then(|n| n.get("updated")).and_then(Value::as_f64),
    }
}

/// `text.trim().slice(0, 2000)` (JavaScript trims the same white space as Rust, UTF-16 units counted).
pub fn clean(text: &str) -> String {
    let t = text.trim();
    let mut units = 0;
    let mut end = t.len();
    for (i, c) in t.char_indices() {
        units += c.len_utf16();
        if units > NOTE_MAX {
            end = i;
            break;
        }
    }
    t[..end].to_string()
}

/// The stored text after saving a note (an empty note is removed) and whether a note is kept.
pub fn save(raw: Option<&str>, id: &str, text: &str, now: f64) -> (String, bool) {
    let mut all = load(raw);
    let c = clean(text);
    let kept = !c.is_empty();
    if kept {
        all.insert(id.to_string(), json!({ "text": c, "updated": now }));
    } else {
        all.remove(id);
    }
    (crate::js::to_value(&Value::Object(all)).to_string(), kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_round_trip() {
        let (s, kept) = save(None, "crypto:BTC", "  acheté pour 3 ans  ", 5.0);
        assert!(kept);
        assert_eq!(s, r#"{"crypto:BTC":{"text":"acheté pour 3 ans","updated":5}}"#);
        assert_eq!(note(Some(&s), "crypto:BTC"), Note { text: "acheté pour 3 ans".into(), updated: Some(5.0) });
        assert_eq!(note(Some(&s), "stock:AAPL"), Note { text: String::new(), updated: None });
        let (s, kept) = save(Some(&s), "crypto:BTC", "   ", 6.0);
        assert!(!kept);
        assert_eq!(s, "{}");
        assert_eq!(note(Some("{oops"), "x").text, "");
        assert_eq!(clean(&"é".repeat(2100)).chars().count(), 2000);
    }
}
