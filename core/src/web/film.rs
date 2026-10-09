//! The home page's scroll « film »: a full-screen scene where luminous particles morph from one Altim line drawing to
//! the next as the page scrolls, one short caption per station. The site's .wasm draws the captions and maps the
//! scroll to the scene (`progress`, `station_at`, `caption_alpha`); the particles are drawn by frontend/motion.js,
//! loaded after the first paint, which morphs between the shapes named by `Station::shape` with the same `morph`.
//! Wording: what the app does, never a promise of performance (the app's disclaimers).

/// One station of the film: the line drawing the particles form, and its caption as runs of text, the runs marked
/// `true` drawn in the accent colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// Shape drawn by the particles (frontend/motion.js `SHAPES`).
    pub shape: &'static str,
    pub caption: &'static [(&'static str, bool)],
    /// What the still illustration shows (reduced motion), for the record: the drawings themselves are decorative.
    pub picture: &'static str,
}

pub const STATIONS: [Station; 7] = [
    Station { shape: "logo", caption: &[("Altim.", true), (" Le marché, lu calmement.", false)], picture: "le logo Altim" },
    Station { shape: "candles", caption: &[("Des cours ", false), ("en direct.", true)], picture: "un graphique en chandeliers" },
    Station { shape: "radar", caption: &[("Un radar qui attend ", false), ("le bon moment.", true)], picture: "un radar qui balaie" },
    Station { shape: "verdict", caption: &[("Un verdict, ", false), ("et pourquoi.", true)], picture: "une balance" },
    Station { shape: "shield", caption: &[("Aucun ordre passé.", true), (" Vos données restent chez vous.", false)], picture: "un bouclier" },
    Station {
        shape: "devices",
        caption: &[("Sur le web, l'iPhone, Android ", false), ("et la montre.", true)],
        picture: "un navigateur, un iPhone et une montre",
    },
    Station { shape: "logo", caption: &[("Conseil indicatif.", false), (" Jamais une certitude.", true)], picture: "le logo Altim" },
];

/// The caption as plain text.
pub fn caption_text(s: &Station) -> String {
    s.caption.iter().map(|(t, _)| *t).collect()
}

/// Hermite smoothstep of `x` between `a` and `b` (0 below `a`, 1 above `b`).
pub fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far the film has been scrolled, 0 → 1: 0 while its top is at or below the top of the screen, 1 once its
/// bottom reaches the bottom of the screen (the sticky scene then scrolls away). `top` and `height` are the section's
/// bounding box, `viewport` the screen height, all in CSS pixels.
pub fn progress(top: f64, height: f64, viewport: f64) -> f64 {
    let run = height - viewport;
    if run.is_nan() || run <= 0.0 || !top.is_finite() {
        return 0.0;
    }
    (-top / run).clamp(0.0, 1.0)
}

/// Position in the film, 0 (first station) → `n - 1` (last), at a progress 0 → 1.
pub fn station_at(p: f64, n: usize) -> f64 {
    p.clamp(0.0, 1.0) * n.saturating_sub(1) as f64
}

/// The station shown at a position: the nearest one.
pub fn nearest(s: f64, n: usize) -> usize {
    (s.round().max(0.0) as usize).min(n.saturating_sub(1))
}

/// The two shapes on screen at position `s` and how far the particles have gone from the first to the second
/// (0 → 1, eased). Each station holds its shape over the middle half of the scroll around it: the morph happens
/// between 25 % and 75 % of the way to the next one. frontend/motion.js `morph` is the same function.
pub fn morph(s: f64, n: usize) -> (usize, usize, f64) {
    if n < 2 {
        return (0, 0, 0.0);
    }
    let s = s.clamp(0.0, (n - 1) as f64);
    let i = (s.floor() as usize).min(n - 2);
    (i, i + 1, smoothstep(0.25, 0.75, s - i as f64))
}

/// Opacity of the caption of station `k` at position `s`: fully shown within 0.18 of its station, gone beyond 0.42,
/// so that two captions never overlap.
pub fn caption_alpha(s: f64, k: usize) -> f64 {
    1.0 - smoothstep(0.18, 0.42, (s - k as f64).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_stations_starting_and_ending_on_the_logo() {
        assert_eq!(STATIONS.len(), 7);
        assert_eq!(STATIONS[0].shape, "logo");
        assert_eq!(STATIONS[6].shape, "logo");
        assert_eq!(caption_text(&STATIONS[0]), "Altim. Le marché, lu calmement.");
        assert_eq!(caption_text(&STATIONS[4]), "Aucun ordre passé. Vos données restent chez vous.");
        assert_eq!(caption_text(&STATIONS[6]), "Conseil indicatif. Jamais une certitude.");
        for s in STATIONS {
            assert!(s.caption.iter().any(|(_, accent)| *accent), "{} sans mot en couleur", s.shape);
            assert!(s.caption.iter().any(|(_, accent)| !*accent), "{} tout en couleur", s.shape);
        }
    }

    #[test]
    fn honest_wording() {
        // No performance promise on the home page's film.
        for s in STATIONS {
            let t = caption_text(&s).to_lowercase();
            for word in ["gagn", "profit", "garanti", "rendement", "sûr", "%"] {
                assert!(!t.contains(word), "« {t} » contient « {word} »");
            }
        }
    }

    #[test]
    fn progress_follows_the_scroll_through_the_section() {
        // A 4000 px section on an 800 px screen: 3200 px of scroll drive the film.
        assert_eq!(progress(100.0, 4000.0, 800.0), 0.0);
        assert_eq!(progress(0.0, 4000.0, 800.0), 0.0);
        assert_eq!(progress(-1600.0, 4000.0, 800.0), 0.5);
        assert_eq!(progress(-3200.0, 4000.0, 800.0), 1.0);
        assert_eq!(progress(-9000.0, 4000.0, 800.0), 1.0);
        // A section no taller than the screen, or a broken box: the first station.
        assert_eq!(progress(-10.0, 800.0, 800.0), 0.0);
        assert_eq!(progress(f64::NAN, 4000.0, 800.0), 0.0);
        assert_eq!(station_at(0.5, 7), 3.0);
        assert_eq!(station_at(2.0, 7), 6.0);
        assert_eq!(nearest(2.49, 7), 2);
        assert_eq!(nearest(2.5, 7), 3);
        assert_eq!(nearest(9.0, 7), 6);
    }

    #[test]
    fn each_station_holds_its_shape_then_morphs() {
        assert_eq!(morph(0.0, 7), (0, 1, 0.0));
        assert_eq!(morph(0.2, 7), (0, 1, 0.0));
        assert_eq!(morph(0.5, 7), (0, 1, 0.5));
        assert_eq!(morph(0.8, 7), (0, 1, 1.0));
        assert_eq!(morph(3.0, 7), (3, 4, 0.0));
        // The last station: the end of the last morph, held.
        assert_eq!(morph(6.0, 7), (5, 6, 1.0));
        assert_eq!(morph(7.5, 7), (5, 6, 1.0));
        assert_eq!(morph(-1.0, 7), (0, 1, 0.0));
        assert_eq!(morph(3.0, 1), (0, 0, 0.0));
    }

    #[test]
    fn one_caption_at_a_time() {
        assert_eq!(caption_alpha(2.0, 2), 1.0);
        assert_eq!(caption_alpha(2.15, 2), 1.0);
        assert_eq!(caption_alpha(2.5, 2), 0.0);
        assert_eq!(caption_alpha(2.5, 3), 0.0);
        // Never two captions visible together.
        for i in 0..=600 {
            let s = i as f64 / 100.0;
            let shown = (0..7).filter(|k| caption_alpha(s, *k) > 0.0).count();
            assert!(shown <= 1, "{s}: {shown} légendes");
        }
    }
}
