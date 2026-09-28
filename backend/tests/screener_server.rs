//! Server part of the selection (former web/test/selection.test.ts, universe of the largest companies).
use altim::screener::{clean_name, parse_listed, sector_fr};
use serde_json::json;

#[test]
fn largest_companies_one_share_class_each() {
    let rows = json!([
        { "symbol": "GOOGL", "name": "Alphabet Inc. Class A Common Stock", "marketCap": "4187062800000.00", "sector": "Technology" },
        { "symbol": "GOOG", "name": "Alphabet Inc. Class C Capital Stock", "marketCap": "4146092300000.00", "sector": "Technology" },
        { "symbol": "AAPL", "name": "Apple Inc. Common Stock", "marketCap": "4902476945600.00", "sector": "Technology" },
        { "symbol": "BRK/B", "name": "Berkshire Hathaway Inc.", "marketCap": "1000000000000", "sector": "Finance" },
        { "symbol": "XYZ^", "name": "Bad symbol", "marketCap": "5", "sector": "" },
        { "symbol": "ZERO", "name": "No cap", "marketCap": "", "sector": "Energy" },
    ]);
    let l = parse_listed(&json!({ "data": { "rows": rows } }), 10);
    assert_eq!(l.iter().map(|x| x.symbol.as_str()).collect::<Vec<_>>(), ["AAPL", "GOOGL", "BRK-B"]);
    assert_eq!(l[0].name, "Apple Inc.");
    assert_eq!(l[2].sector, "Finance");
    assert_eq!(clean_name("NVIDIA Corporation Common Stock"), "NVIDIA Corporation");
    assert_eq!(
        clean_name("Taiwan Semiconductor Manufacturing Company Ltd. American Depositary Shares"),
        "Taiwan Semiconductor Manufacturing Company Ltd."
    );
    assert_eq!(sector_fr("Health Care"), "Santé");
    assert_eq!(sector_fr("Unknown"), "Unknown");
    // No sector → "Autre"; a list larger than asked is cut.
    let many: Vec<_> =
        (0..20).map(|i| json!({ "symbol": format!("A{i}"), "name": format!("Co {i}"), "marketCap": format!("{}", 100 - i) })).collect();
    let l = parse_listed(&json!({ "data": { "rows": many } }), 5);
    assert_eq!(l.len(), 5);
    assert_eq!(l[0].sector, "Autre");
}
