//! Markdown helpers for summaries.

/// Escape a value for a markdown table cell.
pub fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Render a markdown table.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push('|');
    for h in headers {
        out.push(' ');
        out.push_str(&cell(h));
        out.push_str(" |");
    }
    out.push_str("\n|");
    for _ in headers {
        out.push_str("---|");
    }
    out.push('\n');
    for r in rows {
        out.push('|');
        for v in r {
            out.push(' ');
            out.push_str(&cell(v));
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}

/// Format a number with thousands separators and fixed decimals.
pub fn num(x: f64, decimals: usize) -> String {
    let s = format!("{x:.decimals$}");
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let (sign, digits) = match int.strip_prefix('-') {
        Some(d) => ("-", d.to_string()),
        None => ("", int),
    };
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_and_tables() {
        assert_eq!(num(1234567.891, 1), "1,234,567.9");
        assert_eq!(num(-1000.0, 0), "-1,000");
        assert_eq!(num(12.0, 2), "12.00");
        let t = table(&["A", "B"], &[vec!["1".into(), "x|y".into()]]);
        assert!(t.contains("| 1 | x\\|y |"));
    }
}
