//! Minimal RTF-to-text conversion for the DPH guide chapters that mass.gov
//! publishes as RTF. Keeps body text, paragraph and table structure; drops
//! font, color, style, list, picture, and field-instruction groups.

const SKIP_DESTINATIONS: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "header",
    "headerl",
    "headerr",
    "headerf",
    "footer",
    "footerl",
    "footerr",
    "footerf",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "generator",
    "xmlnstbl",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "mmathPr",
    "fldinst",
    "object",
    "nonshppict",
    "filetbl",
    "revtbl",
    "pgdsctbl",
    "footnote",
    "annotation",
    "bkmkstart",
    "bkmkend",
    "xe",
    "tc",
    "listtext",
    "pntext",
    "pntxta",
    "pntxtb",
    "pn",
    "sp",
    "sv",
    "shpinst",
    "defchp",
    "defpap",
    "wgrffmtfilter",
    "factoidname",
    "docvar",
    "template",
];

/// Windows-1252 byte to char.
fn cp1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => char::from_u32(u32::from(HIGH[(b - 0x80) as usize])).unwrap_or('?'),
        _ => b as char,
    }
}

/// Convert RTF bytes to plain text (one paragraph per line; table cells
/// separated by " | ").
pub fn to_text(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len() / 4);
    // Per group: (skip, unicode skip count).
    let mut stack: Vec<(bool, usize)> = vec![(false, 1)];
    let mut i = 0;
    let mut pending_skip = 0usize; // chars to skip after \uN
    let n = input.len();
    while i < n {
        let c = input[i];
        let (skip, uc) = *stack.last().unwrap_or(&(false, 1));
        match c {
            b'{' => {
                stack.push((skip, uc));
                i += 1;
                // "{\*" marks an ignorable destination.
                if input.get(i) == Some(&b'\\') && input.get(i + 1) == Some(&b'*') {
                    if let Some(top) = stack.last_mut() {
                        top.0 = true;
                    }
                    i += 2;
                }
            }
            b'}' => {
                stack.pop();
                if stack.is_empty() {
                    stack.push((false, 1));
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&next) = input.get(i) else { break };
                if next.is_ascii_alphabetic() {
                    let start = i;
                    while i < n && input[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&input[start..i]).unwrap_or("");
                    let mut num: Option<i64> = None;
                    let nstart = i;
                    if i < n && (input[i] == b'-' || input[i].is_ascii_digit()) {
                        i += 1;
                        while i < n && input[i].is_ascii_digit() {
                            i += 1;
                        }
                        num = std::str::from_utf8(&input[nstart..i])
                            .ok()
                            .and_then(|s| s.parse().ok());
                    }
                    if i < n && input[i] == b' ' {
                        i += 1;
                    }
                    if word == "bin" {
                        i += num.unwrap_or(0).max(0) as usize;
                        continue;
                    }
                    if SKIP_DESTINATIONS.contains(&word) {
                        if let Some(top) = stack.last_mut() {
                            top.0 = true;
                        }
                        continue;
                    }
                    if word == "uc" {
                        if let Some(top) = stack.last_mut() {
                            top.1 = num.unwrap_or(1).max(0) as usize;
                        }
                        continue;
                    }
                    if skip {
                        continue;
                    }
                    if pending_skip > 0 {
                        pending_skip = 0;
                    }
                    match word {
                        "par" | "line" | "sect" | "page" | "row" => out.push('\n'),
                        "tab" => out.push('\t'),
                        "cell" => out.push_str(" | "),
                        "emdash" => out.push('—'),
                        "endash" => out.push('–'),
                        "lquote" => out.push('‘'),
                        "rquote" => out.push('’'),
                        "ldblquote" => out.push('“'),
                        "rdblquote" => out.push('”'),
                        "bullet" => out.push('•'),
                        "emspace" | "enspace" | "qmspace" => out.push(' '),
                        "u" => {
                            if let Some(v) = num {
                                let v = if v < 0 { v + 65536 } else { v };
                                if let Some(ch) = char::from_u32(v as u32) {
                                    out.push(ch);
                                }
                                pending_skip = uc;
                            }
                        }
                        _ => {}
                    }
                } else {
                    i += 1;
                    match next {
                        b'\'' => {
                            let hex = input.get(i..i + 2).unwrap_or(b"3f");
                            i += 2;
                            if skip {
                                continue;
                            }
                            if pending_skip > 0 {
                                pending_skip -= 1;
                                continue;
                            }
                            if let Ok(b) =
                                u8::from_str_radix(std::str::from_utf8(hex).unwrap_or("3f"), 16)
                            {
                                out.push(cp1252(b));
                            }
                        }
                        b'~' if !skip => out.push(' '),
                        b'_' if !skip => out.push('-'),
                        b'{' | b'}' | b'\\' if !skip => out.push(next as char),
                        b'\n' | b'\r' if !skip => out.push('\n'),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                i += 1;
                if skip {
                    continue;
                }
                if pending_skip > 0 {
                    pending_skip -= 1;
                    continue;
                }
                out.push(c as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_rtf() {
        let rtf = br"{\rtf1\ansi{\fonttbl{\f0 Times;}}{\*\generator x;}\pard Section 1\par
ABOUT THE DISEASE\par A. Etiologic Agent\par The bacteria\rquote s range is 6\endash 72 hours\u8805?100.\par
{\pict\pngblip 89504e47}\trowd\cell A\cell B\cell\row}";
        let t = to_text(rtf);
        assert!(t.contains("Section 1\nABOUT THE DISEASE\n"), "{t}");
        assert!(t.contains("bacteria’s range is 6–72 hours≥100."), "{t}");
        assert!(!t.contains("Times") && !t.contains("89504e47"), "{t}");
        assert!(t.contains(" | A | B | "), "{t}");
    }
}
