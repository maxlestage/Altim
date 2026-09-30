//! Calendar events (`/api/calendar`, the decision's agenda): the JSON contract shared by the server and the browser.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Macro,
    Earnings,
    Dividend,
    Split,
    Ipo,
    CentralBank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// Rate decisions, statements, minutes (Fed, ECB…).
    TauxDirecteurs,
    Inflation,
    Emploi,
    Pib,
    /// Activity and confidence surveys (ISM, retail sales, consumer sentiment).
    Activite,
    /// Speeches of a central bank's chair.
    Discours,
    Resultats,
    Dividende,
    Split,
    Ipo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Importance {
    High,
    Medium,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    /// Exact instant (ms) when the source gives a time, otherwise 00:00 UTC of `day`.
    pub date: i64,
    /// Day of the event, "YYYY-MM-DD" (Paris time when the time is known).
    pub day: String,
    /// Display time: "14:30" (Paris time) or, for earnings, "avant l'ouverture" / "après la clôture".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    pub kind: EventKind,
    pub category: Category,
    pub importance: Importance,
    /// French title.
    pub title: String,
    /// The event's name as the source publishes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// French country name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Extra facts from the same source (payment date and amount, split ratio, IPO price range…), in French.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Cross-check between sources (e.g. Nasdaq and the Fed disagreeing on a date), in French.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub source: String,
    pub url: String,
}
