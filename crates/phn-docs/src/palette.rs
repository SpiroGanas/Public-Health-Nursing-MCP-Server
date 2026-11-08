//! Accessible colors for charts and maps.
//!
//! The categorical order was validated for color-vision deficiency (adjacent
//! pairs ΔE ≥ 9 under protan/deutan/tritan simulation in OKLab ×100) and
//! normal vision (adjacent ΔE ≥ 19.6) against a white document background.
//! Three slots (aqua, yellow, magenta) are below 3:1 contrast on white, so
//! every chart also carries direct labels, a data table, and alt text: color
//! never carries meaning alone (WCAG 1.4.1, 1.4.11).
//!
//! Rules for callers:
//! * assign categorical colors in this fixed order, never cycled — fold a
//!   ninth series into "Other" or use small multiples;
//! * scatter-like and map forms where every pair can touch use at most the
//!   first three slots;
//! * sequential (magnitude) encodings use the one-hue [`SEQUENTIAL_BLUE`]
//!   ramp, light to dark; diverging encodings use blue ↔ red around a gray
//!   midpoint;
//! * text is drawn in [`INK`]/[`INK_SECONDARY`], never in a series color.

/// Categorical series colors, in assignment order.
pub const CATEGORICAL: [&str; 8] = [
    "#2a78d6", // blue
    "#eb6834", // orange
    "#1baf7a", // aqua
    "#eda100", // yellow
    "#e87ba4", // magenta
    "#008300", // green
    "#6250d6", // violet
    "#e34948", // red
];

/// Names of the categorical colors (for alt text and legends).
pub const CATEGORICAL_NAMES: [&str; 8] = [
    "blue", "orange", "aqua", "yellow", "magenta", "green", "violet", "red",
];

/// Marker shapes paired with the categorical slots (secondary encoding).
pub const MARKERS: [Marker; 8] = [
    Marker::Circle,
    Marker::Square,
    Marker::Triangle,
    Marker::Diamond,
    Marker::Circle,
    Marker::Square,
    Marker::Triangle,
    Marker::Diamond,
];

/// A point marker shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Circle,
    Square,
    Triangle,
    Diamond,
}

/// One-hue sequential ramp (light → dark) for magnitudes, heatmaps, and
/// choropleth maps. Index 0 is the lightest step.
pub const SEQUENTIAL_BLUE: [&str; 13] = [
    "#cde2fb", "#b7d3f6", "#9ec5f4", "#86b6ef", "#6da7ec", "#5598e7", "#3987e5", "#2a78d6",
    "#256abf", "#1c5cab", "#184f95", "#104281", "#0d366b",
];

/// Diverging ramp: blue arm (dark → light), gray midpoint, red arm (light → dark).
pub const DIVERGING_BLUE_RED: [&str; 7] = [
    "#184f95", "#5598e7", "#b7d3f6", "#f0efec", "#f4b8b5", "#e34948", "#a32d2c",
];

/// Primary text and axis-label ink.
pub const INK: &str = "#0b0b0b";
/// Secondary text (subtitles, notes).
pub const INK_SECONDARY: &str = "#52514e";
/// Muted ink for tick labels.
pub const INK_MUTED: &str = "#6b6a66";
/// Hairline gridlines.
pub const GRID: &str = "#e1e0d9";
/// Baseline and axis lines.
pub const AXIS: &str = "#c3c2b7";
/// Chart background (documents are printed on white).
pub const SURFACE: &str = "#ffffff";
/// Fill for suppressed or missing values (with a hatch pattern).
pub const SUPPRESSED: &str = "#d9d8d2";
/// Special-cause or alert marks on QI charts (paired with a label, never alone).
pub const ALERT: &str = "#d03b3b";

/// Categorical color for series `i` (panics past the eighth slot by design:
/// callers must fold extra series into "Other").
pub fn categorical(i: usize) -> &'static str {
    CATEGORICAL[i]
}

/// Pick `n` evenly spaced steps (light → dark) from the sequential ramp,
/// skipping the lightest step so every class is visible on white.
pub fn sequential_steps(n: usize) -> Vec<&'static str> {
    let n = n.clamp(1, SEQUENTIAL_BLUE.len() - 1);
    if n == 1 {
        return vec![SEQUENTIAL_BLUE[7]];
    }
    let lo = 1.0;
    let hi = (SEQUENTIAL_BLUE.len() - 1) as f64;
    (0..n)
        .map(|i| {
            let t = lo + (hi - lo) * i as f64 / (n - 1) as f64;
            SEQUENTIAL_BLUE[t.round() as usize]
        })
        .collect()
}

/// Parse `#rrggbb` into 0–255 channels.
pub fn rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

/// WCAG 2.x relative luminance.
pub fn luminance(hex: &str) -> f64 {
    let Some((r, g, b)) = rgb(hex) else {
        return 0.0;
    };
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG 2.x contrast ratio between two colors.
pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Text color (black or white) with the better contrast on a fill.
pub fn label_on(fill: &str) -> &'static str {
    if contrast(fill, "#ffffff") >= contrast(fill, INK) {
        "#ffffff"
    } else {
        INK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_and_axes_meet_contrast_minimums() {
        assert!(contrast(INK, SURFACE) >= 7.0);
        assert!(contrast(INK_SECONDARY, SURFACE) >= 4.5);
        assert!(contrast(INK_MUTED, SURFACE) >= 4.5);
        assert!(contrast(ALERT, SURFACE) >= 4.5);
    }

    #[test]
    fn sequential_ramp_darkens_monotonically() {
        for w in SEQUENTIAL_BLUE.windows(2) {
            assert!(luminance(w[0]) > luminance(w[1]), "{} vs {}", w[0], w[1]);
        }
        let five = sequential_steps(5);
        assert_eq!(five.len(), 5);
        assert_eq!(five[4], SEQUENTIAL_BLUE[12]);
        assert_ne!(five[0], SEQUENTIAL_BLUE[0]);
    }

    #[test]
    fn labels_on_fills_are_readable() {
        for c in CATEGORICAL.iter().chain(SEQUENTIAL_BLUE.iter()) {
            assert!(contrast(label_on(c), c) >= 2.5, "{c}");
        }
        assert_eq!(rgb("#ff0080"), Some((255, 0, 128)));
        assert!(rgb("red").is_none());
    }
}
