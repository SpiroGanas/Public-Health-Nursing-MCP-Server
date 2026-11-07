//! Bundled fonts and text measurement.

use std::sync::LazyLock;

use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::{GlyphMetrics, Metrics};
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// DejaVu Sans.
pub static REGULAR: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
/// DejaVu Sans Bold.
pub static BOLD: &[u8] = include_bytes!("../fonts/DejaVuSans-Bold.ttf");

/// A bundled font with its character map and metrics.
pub struct Face {
    charmap: Charmap<'static>,
    glyphs: GlyphMetrics<'static>,
    /// Font-wide metrics in font units.
    pub metrics: Metrics,
}

impl Face {
    fn load(data: &'static [u8]) -> Self {
        let font = FontRef::new(data).expect("bundled font parses");
        Self {
            charmap: font.charmap(),
            glyphs: font.glyph_metrics(Size::unscaled(), LocationRef::default()),
            metrics: font.metrics(Size::unscaled(), LocationRef::default()),
        }
    }

    /// Glyph id for a character, if the font has one.
    pub fn glyph(&self, c: char) -> Option<u16> {
        self.charmap
            .map(c)
            .and_then(|g| u16::try_from(g.to_u32()).ok())
            .filter(|&g| g != 0)
    }

    /// Advance width of a glyph in font units.
    pub fn advance(&self, glyph: u16) -> f32 {
        self.glyphs
            .advance_width(GlyphId::new(u32::from(glyph)))
            .unwrap_or_else(|| self.units_per_em() / 2.0)
    }

    /// Units per em.
    pub fn units_per_em(&self) -> f32 {
        f32::from(self.metrics.units_per_em)
    }
}

/// Regular and bold faces.
pub struct Faces {
    pub regular: Face,
    pub bold: Face,
}

pub static FACES: LazyLock<Faces> = LazyLock::new(|| Faces {
    regular: Face::load(REGULAR),
    bold: Face::load(BOLD),
});

/// The face for a weight.
pub fn face(bold: bool) -> &'static Face {
    if bold { &FACES.bold } else { &FACES.regular }
}

/// True when the bundled fonts can draw `c`.
pub fn has_glyph(c: char) -> bool {
    FACES.regular.glyph(c).is_some()
}

/// Advance width of a character in font units, with a fallback for missing glyphs.
pub fn char_advance(face: &Face, c: char) -> f32 {
    match face.glyph(c) {
        Some(g) => face.advance(g),
        None => face.units_per_em() / 2.0,
    }
}

/// Width of `text` in points at `size`.
pub fn text_width(text: &str, bold: bool, size: f32) -> f32 {
    let f = face(bold);
    text.chars().map(|c| char_advance(f, c)).sum::<f32>() * size / f.units_per_em()
}

/// Greedy word wrap to `max_width` points. Very long words are broken.
pub fn wrap(text: &str, bold: bool, size: f32, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if text_width(&candidate, bold, size) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // Break a word that is wider than the line.
            let mut piece = String::new();
            for c in word.chars() {
                piece.push(c);
                if text_width(&piece, bold, size) > max_width && piece.chars().count() > 1 {
                    let last = piece.pop().expect("non-empty");
                    lines.push(std::mem::take(&mut piece));
                    piece.push(last);
                }
            }
            line = piece;
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_wraps() {
        assert!(text_width("WWW", false, 10.0) > text_width("iii", false, 10.0));
        assert!(text_width("abc", true, 10.0) > text_width("abc", false, 10.0));
        let lines = wrap(
            "the quick brown fox jumps over the lazy dog",
            false,
            12.0,
            100.0,
        );
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| text_width(l, false, 12.0) <= 100.0));
        assert!(has_glyph('ñ'));
        assert!(has_glyph('ệ')); // Vietnamese
        assert!(!has_glyph('中'));
    }
}
