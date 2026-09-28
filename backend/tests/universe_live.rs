//! Live comparison with the TypeScript universe (network): first `bun parity/universe.ts <dir>`, then
//! `ALTIM_UNIVERSE_DIR=<dir> cargo test --test universe_live -- --ignored --nocapture`.
//! From the same raw responses the lists must be identical; the live searches are compared and the differences
//! printed (both sides fetch at different moments, the upstream data can move in between).
use altim::universe::{UniverseEntry, build_crypto, build_stocks, crypto_universe, parse, search_all, stock_universe};
use serde_json::{Value, json};

const QUERIES: [&str; 3] = ["bit", "apple", "nvd"];

fn dir() -> String {
    std::env::var("ALTIM_UNIVERSE_DIR").expect("ALTIM_UNIVERSE_DIR (bun parity/universe.ts <dir>)")
}

fn searches(c: &[UniverseEntry], s: &[UniverseEntry]) -> Value {
    let mut out = serde_json::Map::new();
    for q in QUERIES {
        let hits: Vec<Value> = search_all(c, s, q, 20).iter().map(|h| json!([h.kind.as_str(), h.e.0, h.e.1, h.e.2, h.e.3])).collect();
        out.insert(q.into(), Value::Array(hits));
    }
    Value::Object(out)
}

fn report(label: &str, rust: &Value, ts: &Value) -> usize {
    let mut diffs = 0;
    for q in QUERIES {
        let (r, t) = (rust[q].as_array().unwrap(), ts[q].as_array().unwrap());
        let fmt = |v: &Value| format!("{}:{} ({}, rang {})", v[0].as_str().unwrap(), v[1].as_str().unwrap(), v[2].as_str().unwrap(), v[3]);
        println!("[{label}] « {q} » : {} résultats Rust, {} TS", r.len(), t.len());
        for i in 0..r.len().max(t.len()) {
            let (a, b) = (r.get(i).map(fmt).unwrap_or("—".into()), t.get(i).map(fmt).unwrap_or("—".into()));
            if a != b {
                diffs += 1;
                println!("  #{i}: Rust {a} | TS {b}");
            }
        }
    }
    diffs
}

#[tokio::test]
#[ignore]
async fn same_lists_from_the_same_raw_responses() {
    let dir = dir();
    let raw = |n: &str| std::fs::read_to_string(format!("{dir}/raw/{n}.txt")).unwrap_or_default();
    let json = |n: &str| serde_json::from_str::<Value>(&raw(n)).unwrap_or(Value::Null);
    let gecko: Vec<_> = ["gecko1", "gecko2", "gecko3", "gecko4"].iter().flat_map(|p| parse::gecko(&json(p)).unwrap_or_default()).collect();
    let names: Vec<(String, String)> = parse::coinbase_names(&json("coinbaseNames"))
        .unwrap_or_default()
        .into_iter()
        .chain(parse::kucoin_names(&json("kucoinNames")).unwrap_or_default())
        .chain(parse::gecko_names(&json("geckoNames")).unwrap_or_default())
        .collect();
    let exchanges = [
        parse::okx(&json("okx")).unwrap_or_default(),
        parse::coinbase(&json("coinbase")).unwrap_or_default(),
        parse::kraken(&json("kraken")).unwrap_or_default(),
        parse::kucoin(&json("kucoin")).unwrap_or_default(),
        parse::gate(&json("gate")).unwrap_or_default(),
    ];
    let crypto = build_crypto(&exchanges, &gecko, &names);
    let stock = build_stocks(
        &[parse::nasdaq_directory(&raw("nasdaq")).unwrap_or_default(), parse::nasdaq_directory(&raw("other")).unwrap_or_default()],
        &parse::screener(&json("screener")).unwrap_or_default(),
    );
    let ts: Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/ts_from_raw.json")).unwrap()).unwrap();
    let ts_crypto: Vec<UniverseEntry> = serde_json::from_value(ts["crypto"].clone()).unwrap();
    let ts_stock: Vec<UniverseEntry> = serde_json::from_value(ts["stock"].clone()).unwrap();
    println!("depuis les mêmes réponses : {} / {} cryptos, {} / {} actions (Rust / TS)", crypto.len(), ts_crypto.len(), stock.len(), ts_stock.len());
    for (i, (a, b)) in crypto.iter().zip(&ts_crypto).enumerate() {
        assert_eq!(a, b, "crypto #{i}");
    }
    for (i, (a, b)) in stock.iter().zip(&ts_stock).enumerate() {
        assert_eq!(a, b, "action #{i}");
    }
    assert_eq!((crypto.len(), stock.len()), (ts_crypto.len(), ts_stock.len()));
    assert_eq!(report("mêmes réponses", &searches(&crypto, &stock), &ts["searches"]), 0);
}

#[tokio::test]
#[ignore]
async fn live_searches_like_typescript() {
    let ts: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/ts_live.json", dir())).unwrap()).unwrap();
    let (c, s) = tokio::join!(crypto_universe(), stock_universe());
    let (c, s) = (c.expect("cryptos"), s.expect("actions"));
    println!(
        "en direct : {} cryptos Rust / {} TS, {} actions Rust / {} TS",
        c.len(),
        ts["crypto"].as_array().unwrap().len(),
        s.len(),
        ts["stock"].as_array().unwrap().len()
    );
    let ours = json!({ "crypto": *c, "stock": *s, "searches": searches(&c, &s) });
    std::fs::write(format!("{}/rust_live.json", dir()), ours.to_string()).unwrap();
    let diffs = report("en direct", &ours["searches"], &ts["searches"]);
    println!("{diffs} différence(s) sur les recherches en direct");
}
