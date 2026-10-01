//! `serde_json::from_value` for the browser, lighter in the .wasm: the same reading of a parsed `Value` as serde_json's
//! deserializer for `&Value` (numbers, options, enums, maps and their keys, errors on the wrong types), except that a
//! struct is read from a JSON object only. serde_json also accepts an array for a struct (its fields in order), which
//! no answer of the server or saved state uses; dropping it leaves out of the .wasm the second reader that serde
//! generates for every struct (`visit_seq`), about a third of their code.
use serde::de::value::BorrowedStrDeserializer;
use serde::de::{self, DeserializeOwned, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, Unexpected, VariantAccess, Visitor};
use serde::forward_to_deserialize_any;
use serde_json::{Error, Map, Number, Value};

/// `serde_json::from_value(v.clone())`, without the clone (see the module).
pub fn from_value<T: DeserializeOwned>(v: &Value) -> Result<T, Error> {
    T::deserialize(De(v))
}

#[derive(Clone, Copy)]
struct De<'de>(&'de Value);

fn unexpected(v: &Value) -> Unexpected<'_> {
    match v {
        Value::Null => Unexpected::Unit,
        Value::Bool(b) => Unexpected::Bool(*b),
        Value::Number(n) => match (n.as_u64(), n.as_i64(), n.as_f64()) {
            (Some(u), _, _) => Unexpected::Unsigned(u),
            (_, Some(i), _) => Unexpected::Signed(i),
            (_, _, Some(f)) => Unexpected::Float(f),
            _ => Unexpected::Other("number"),
        },
        Value::String(s) => Unexpected::Str(s),
        Value::Array(_) => Unexpected::Seq,
        Value::Object(_) => Unexpected::Map,
    }
}

fn invalid<'de, V: Visitor<'de>>(v: &Value, visitor: &V) -> Error {
    de::Error::invalid_type(unexpected(v), visitor)
}

/// serde_json's `Number::deserialize_any`: an unsigned, a signed or a float, as stored.
fn number<'de, V: Visitor<'de>>(n: &Number, visitor: V) -> Result<V::Value, Error> {
    if let Some(u) = n.as_u64() {
        visitor.visit_u64(u)
    } else if let Some(i) = n.as_i64() {
        visitor.visit_i64(i)
    } else {
        visitor.visit_f64(n.as_f64().unwrap_or(f64::NAN))
    }
}

fn seq<'de, V: Visitor<'de>>(a: &'de [Value], visitor: V) -> Result<V::Value, Error> {
    let len = a.len();
    let mut s = Seq(a.iter());
    let out = visitor.visit_seq(&mut s)?;
    if s.0.len() == 0 { Ok(out) } else { Err(de::Error::invalid_length(len, &"fewer elements in array")) }
}

fn map<'de, V: Visitor<'de>>(m: &'de Map<String, Value>, visitor: V) -> Result<V::Value, Error> {
    let len = m.len();
    let mut access = Entries { iter: m.iter(), value: None };
    let out = visitor.visit_map(&mut access)?;
    if access.iter.len() == 0 { Ok(out) } else { Err(de::Error::invalid_length(len, &"fewer elements in map")) }
}

macro_rules! numbers {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            match self.0 {
                Value::Number(n) => number(n, visitor),
                v => Err(invalid(v, &visitor)),
            }
        }
    )*};
}

impl<'de> de::Deserializer<'de> for De<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::Number(n) => number(n, visitor),
            Value::String(s) => visitor.visit_borrowed_str(s),
            Value::Array(a) => seq(a, visitor),
            Value::Object(m) => map(m, visitor),
        }
    }

    numbers!(deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64 deserialize_u8 deserialize_u16 deserialize_u32
        deserialize_u64 deserialize_f32 deserialize_f64);

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(self, _name: &'static str, _variants: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Object(m) => {
                let mut it = m.iter();
                match (it.next(), it.next()) {
                    (Some((variant, value)), None) => visitor.visit_enum(Variant { variant, value: Some(value) }),
                    _ => Err(de::Error::invalid_value(Unexpected::Map, &"map with a single key")),
                }
            }
            Value::String(variant) => visitor.visit_enum(Variant { variant, value: None }),
            v => Err(de::Error::invalid_type(unexpected(v), &"string or map")),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Bool(b) => visitor.visit_bool(*b),
            v => Err(invalid(v, &visitor)),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::String(s) => visitor.visit_borrowed_str(s),
            v => Err(invalid(v, &visitor)),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::String(s) => visitor.visit_borrowed_str(s),
            Value::Array(a) => seq(a, visitor),
            v => Err(invalid(v, &visitor)),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            v => Err(invalid(v, &visitor)),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Array(a) => seq(a, visitor),
            v => Err(invalid(v, &visitor)),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(self, _name: &'static str, _len: usize, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Object(m) => map(m, visitor),
            v => Err(invalid(v, &visitor)),
        }
    }

    /// From an object only (see the module).
    fn deserialize_struct<V: Visitor<'de>>(self, _name: &'static str, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}

struct Seq<'de>(std::slice::Iter<'de, Value>);

impl<'de> SeqAccess<'de> for Seq<'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Error> {
        self.0.next().map(|v| seed.deserialize(De(v))).transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct Entries<'de> {
    iter: serde_json::map::Iter<'de>,
    value: Option<&'de Value>,
}

impl<'de> MapAccess<'de> for Entries<'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        match self.iter.next() {
            Some((k, v)) => {
                self.value = Some(v);
                seed.deserialize(Key(k)).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, Error> {
        match self.value.take() {
            Some(v) => seed.deserialize(De(v)),
            None => Err(de::Error::custom("value is missing")),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

/// A key of an object: a string, or a number written as one (like serde_json's map keys).
struct Key<'de>(&'de str);

macro_rules! numeric_keys {
    ($($method:ident $visit:ident $t:ty;)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            match self.0.parse::<$t>() {
                Ok(n) if self.0.starts_with(|c: char| c.is_ascii_digit() || c == '-') => visitor.$visit(n),
                _ => Err(de::Error::custom("expected numeric key")),
            }
        }
    )*};
}

impl<'de> de::Deserializer<'de> for Key<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_str(self.0)
    }

    numeric_keys! {
        deserialize_i8 visit_i8 i8; deserialize_i16 visit_i16 i16; deserialize_i32 visit_i32 i32; deserialize_i64 visit_i64 i64;
        deserialize_u8 visit_u8 u8; deserialize_u16 visit_u16 u16; deserialize_u32 visit_u32 u32; deserialize_u64 visit_u64 u64;
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(self, name: &'static str, variants: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        BorrowedStrDeserializer::<Error>::new(self.0).deserialize_enum(name, variants, visitor)
    }

    forward_to_deserialize_any! {
        bool i128 u128 f32 f64 char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
        ignored_any
    }
}

struct Variant<'de> {
    variant: &'de str,
    value: Option<&'de Value>,
}

impl<'de> EnumAccess<'de> for Variant<'de> {
    type Error = Error;
    type Variant = VariantValue<'de>;

    fn variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<(S::Value, Self::Variant), Error> {
        let name = BorrowedStrDeserializer::<Error>::new(self.variant);
        seed.deserialize(name).map(|v| (v, VariantValue(self.value)))
    }
}

struct VariantValue<'de>(Option<&'de Value>);

impl<'de> VariantAccess<'de> for VariantValue<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.0 {
            Some(v) => de::Deserialize::deserialize(De(v)),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        match self.0 {
            Some(v) => seed.deserialize(De(v)),
            None => Err(de::Error::invalid_type(Unexpected::UnitVariant, &"newtype variant")),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Some(Value::Array(a)) if a.is_empty() => visitor.visit_unit(),
            Some(Value::Array(a)) => seq(a, visitor),
            Some(v) => Err(de::Error::invalid_type(unexpected(v), &"tuple variant")),
            None => Err(de::Error::invalid_type(Unexpected::UnitVariant, &"tuple variant")),
        }
    }

    fn struct_variant<V: Visitor<'de>>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Some(Value::Object(m)) => map(m, visitor),
            Some(v) => Err(de::Error::invalid_type(unexpected(v), &"struct variant")),
            None => Err(de::Error::invalid_type(Unexpected::UnitVariant, &"struct variant")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::from_value;

    /// Read with this module and with serde_json: the same value, written back the same.
    fn same<T: serde::de::DeserializeOwned + Serialize>(v: &serde_json::Value) {
        let ours: T = from_value(v).unwrap();
        let theirs: T = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(serde_json::to_value(&ours).unwrap(), serde_json::to_value(&theirs).unwrap());
    }

    fn sample(f: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/../backend/tests/samples/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    }

    #[test]
    fn real_answers_read_as_serde_json_reads_them() {
        for f in ["decision-btc.json", "decision-aapl.json", "decision-guidance.json"] {
            same::<crate::engine::decision_types::Decision>(&sample(f));
        }
        same::<crate::web::bot::BotReport>(&sample("bot.json"));
        same::<crate::engine::strategies::StrategiesReport>(&sample("strategies-btc.json"));
        same::<crate::engine::strategies::StrategiesReport>(&sample("strategies-aapl.json"));
        same::<crate::engine::validation::ValidationReport>(&sample("validation.json"));
    }

    /// Fundamentals (tagged) and Track (flattened details) are read by hand, through a Value: the same values as the
    /// JSON says, written back to the same JSON.
    #[test]
    fn tagged_and_flattened_by_hand() {
        use crate::engine::decision_types::{Decision, Fundamentals};
        for (f, kind) in [("decision-btc.json", "crypto"), ("decision-aapl.json", "stock")] {
            let v = sample(f);
            let d: Decision = from_value(&v).unwrap();
            let back: Decision = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
            assert_eq!(d, back, "{f}");
            match (&d.fundamentals, kind) {
                (Some(Fundamentals::Crypto(_)), "crypto") | (Some(Fundamentals::Stock(_)), "stock") => {}
                (x, _) => panic!("{f}: {x:?}"),
            }
            let t = d.track.as_ref().unwrap();
            let raw = &v["track"];
            assert_eq!(t.trades as u64, raw["trades"].as_u64().unwrap());
            assert_eq!(t.details.regimes.len(), raw["regimes"].as_array().map_or(0, Vec::len), "{f}");
            assert_eq!(t.details.spread_pct, raw["spreadPct"].as_f64().unwrap_or(0.0), "{f}");
        }
        let t = json!({ "period": "p", "trades": 1, "winRate": 50, "maxDrawdown": 1, "totalReturn": 2, "buyAndHold": 3,
            "feesPct": 0.1, "slippagePct": 0.05, "losingStreak": 0, "note": "" });
        let track: crate::engine::decision_types::Track = from_value(&t).unwrap();
        assert_eq!((track.avg_win, track.details.regimes.len(), track.details.expectancy), (None, 0, None));
        assert!(from_value::<crate::engine::decision_types::Track>(&json!({ "period": "p" })).is_err());
        assert!(from_value::<Fundamentals>(&json!({ "kind": "bond" })).is_err());
        assert!(from_value::<Fundamentals>(&json!({})).is_err());
    }

    #[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    enum Kind {
        Crypto,
        Stock,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "kind")]
    enum Tagged {
        A { x: f64 },
        B,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Inner {
        n: u32,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct All {
        f: f64,
        i: i64,
        u: u8,
        big: u64,
        o: Option<String>,
        none: Option<f64>,
        #[serde(default)]
        missing: Vec<u32>,
        k: Kind,
        t: Tagged,
        b: Tagged,
        m: HashMap<String, f64>,
        keys: HashMap<u32, bool>,
        kinds: HashMap<Kind, u8>,
        tuple: (f64, String),
        list: Vec<Option<i32>>,
        unit: (),
        #[serde(flatten)]
        rest: Inner,
    }

    #[test]
    fn every_shape() {
        let v = json!({
            "f": 1, "i": -3, "u": 7, "big": 18_446_744_073_709_551_615u64, "o": "a", "none": null, "k": "stock",
            "t": { "kind": "A", "x": 2.5 }, "b": { "kind": "B" }, "m": { "a": 1.5 }, "keys": { "12": true },
            "kinds": { "crypto": 1 }, "tuple": [1.5, "x"], "list": [1, null, -2], "unit": null, "n": 4, "extra": [1, 2]
        });
        same::<All>(&v);
        assert_eq!(from_value::<All>(&v).unwrap().rest, Inner { n: 4 });
        // Wrong types fail as with serde_json; a struct is read from an object only.
        assert!(from_value::<Inner>(&json!({ "n": "4" })).is_err());
        assert!(from_value::<Inner>(&json!({ "n": -1 })).is_err());
        assert!(from_value::<u8>(&json!(256)).is_err());
        assert!(from_value::<Kind>(&json!("bond")).is_err());
        assert!(from_value::<Inner>(&json!([4])).is_err());
        assert!(serde_json::from_value::<Inner>(json!([4])).is_ok());
        assert!(from_value::<HashMap<u32, bool>>(&json!({ "x": true })).is_err());
    }
}
