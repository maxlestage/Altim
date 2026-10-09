//! Small pure helpers of the app's micro-animations (frontend/src/live.rs): which digits of a price roll on a tick.

/// The text of a new price in runs, each marked `true` when its digits changed from the previous text: the texts are
/// aligned on their right end (a price grows on the left), and only digits roll; separators, signs and the currency
/// stay still. Equal texts, or no previous text, give one still run.
pub fn digit_runs(prev: Option<&str>, next: &str) -> Vec<(String, bool)> {
    let new: Vec<char> = next.chars().collect();
    let old: Vec<char> = prev.map(|p| p.chars().collect()).unwrap_or_default();
    let mut runs: Vec<(String, bool)> = Vec::new();
    for (i, c) in new.iter().enumerate() {
        let from_end = new.len() - 1 - i;
        let before = old.len().checked_sub(1 + from_end).map(|j| old[j]);
        let changed = prev.is_some() && c.is_ascii_digit() && before != Some(*c);
        match runs.last_mut() {
            Some((s, ch)) if *ch == changed => s.push(*c),
            _ => runs.push((c.to_string(), changed)),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(v: &[(&str, bool)]) -> Vec<(String, bool)> {
        v.iter().map(|(s, b)| (s.to_string(), *b)).collect()
    }

    #[test]
    fn only_the_changed_digits_roll() {
        assert_eq!(digit_runs(Some("67 653,51 €"), "67 654,02 €"), r(&[("67 65", false), ("4", true), (",", false), ("02", true), (" €", false)]));
        assert_eq!(digit_runs(Some("1,25 $"), "1,25 $"), r(&[("1,25 $", false)]));
        assert_eq!(digit_runs(None, "1,25 $"), r(&[("1,25 $", false)]));
        // One more digit on the left: the new digit and any digit that moved roll.
        assert_eq!(digit_runs(Some("9,99"), "10,01"), r(&[("10", true), (",", false), ("01", true)]));
        assert_eq!(digit_runs(Some("—"), "12"), r(&[("12", true)]));
        assert_eq!(digit_runs(Some("12"), ""), Vec::<(String, bool)>::new());
    }
}
