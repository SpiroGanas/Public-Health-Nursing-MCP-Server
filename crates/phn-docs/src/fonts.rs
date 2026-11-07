//! Bundled fonts and text measurement.

use std::sync::LazyLock;

/// DejaVu Sans.
pub static REGULAR: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
/// DejaVu Sans Bold.
pub static BOLD: &[u8] = include_bytes!("../fonts/DejaVuSans-Bold.ttf");

/// Parsed font faces (parsing is cheap but done once).
pub struct Faces {
    pub regular: ttf_parser::Face<'static>,
    pub bold: ttf_parser::Face<'static>,
}

pub static FACES: LazyLock<Faces> = LazyLock::new(|| Faces {
    regular: ttf_parser::Face::parse(REGULAR, 0).expect("bundled regular font"),
    bold: ttf_parser::Face::parse(BOLD, 0).expect("bundled bold font"),
});

/// The face for a weight.
pub fn face(bold: bool) -> &'static ttf_parser::Face<'static> {
    if bold { &FACES.bold } else { &FACES.regular }
}

/// Advance width of a character in font units, with a fallback for missing glyphs.
pub fn char_advance(face: &ttf_parser::Face<'_>, c: char) -> u16 {
    face.glyph_index(c)
        .and_then(|g| face.glyph_hor_advance(g))
        .unwrap_or_else(|| face.units_per_em() / 2)
}

/// Width of `text` in points at `size`.
pub fn text_width(text: &str, bold: bool, size: f32) -> f32 {
    let f = face(bold);
    let upem = f32::from(f.units_per_em());
    text.chars()
        .map(|c| f32::from(char_advance(f, c)))
        .sum::<f32>()
        * size
        / upem
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
        assert!(FACES.regular.glyph_index('ñ').is_some());
        assert!(FACES.regular.glyph_index('ệ').is_some()); // Vietnamese
    }
}
