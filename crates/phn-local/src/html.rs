//! HTML reader: text plus the structure accessibility checks need.

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

/// An `<img>` (or `<input type=image>`/`<area>`) element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HtmlImage {
    pub src: String,
    /// `None` when the `alt` attribute is missing; `Some("")` marks decorative.
    pub alt: Option<String>,
}

/// A `<table>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HtmlTable {
    pub has_th: bool,
    pub has_caption: bool,
    pub rows: usize,
    /// Presentation tables (`role="presentation"`/`"none"`) are layout only.
    pub presentational: bool,
}

/// An inline style with explicit colors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HtmlColorStyle {
    pub text: String,
    pub color: String,
    pub background: Option<String>,
}

/// Parsed HTML.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HtmlDocument {
    pub title: Option<String>,
    pub language: Option<String>,
    /// (level, text) for h1–h6 in document order.
    pub headings: Vec<(u8, String)>,
    pub images: Vec<HtmlImage>,
    pub tables: Vec<HtmlTable>,
    pub link_texts: Vec<String>,
    pub colored: Vec<HtmlColorStyle>,
    /// Form controls without an associated label.
    pub unlabeled_inputs: usize,
    pub text: String,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}

fn text_of(e: ElementRef<'_>) -> String {
    e.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse an HTML document.
pub fn parse_html(source: &str) -> HtmlDocument {
    let html = Html::parse_document(source);
    let mut doc = HtmlDocument {
        title: html
            .select(&sel("title"))
            .next()
            .map(text_of)
            .filter(|t| !t.is_empty()),
        language: html
            .select(&sel("html"))
            .next()
            .and_then(|h| h.value().attr("lang"))
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty()),
        ..HtmlDocument::default()
    };
    for h in html.select(&sel("h1, h2, h3, h4, h5, h6")) {
        let level = h.value().name()[1..].parse::<u8>().unwrap_or(1);
        doc.headings.push((level, text_of(h)));
    }
    for img in html.select(&sel("img, input[type=image], area")) {
        doc.images.push(HtmlImage {
            src: img
                .value()
                .attr("src")
                .or(img.value().attr("href"))
                .unwrap_or("")
                .to_string(),
            alt: img
                .value()
                .attr("alt")
                .map(|a| a.trim().to_string())
                .or_else(|| img.value().attr("aria-label").map(|a| a.trim().to_string())),
        });
    }
    for t in html.select(&sel("table")) {
        let role = t.value().attr("role").unwrap_or("");
        doc.tables.push(HtmlTable {
            has_th: t.select(&sel("th")).next().is_some(),
            has_caption: t.select(&sel("caption")).next().is_some()
                || t.value().attr("aria-label").is_some()
                || t.value().attr("aria-labelledby").is_some(),
            rows: t.select(&sel("tr")).count(),
            presentational: role == "presentation" || role == "none",
        });
    }
    for a in html.select(&sel("a[href]")) {
        let label = a
            .value()
            .attr("aria-label")
            .map(str::to_string)
            .unwrap_or_else(|| text_of(a));
        doc.link_texts.push(label);
    }
    let labeled_ids: Vec<String> = html
        .select(&sel("label[for]"))
        .filter_map(|l| l.value().attr("for").map(str::to_string))
        .collect();
    for input in html.select(&sel("input, select, textarea")) {
        let v = input.value();
        let ty = v.attr("type").unwrap_or("text");
        if matches!(ty, "hidden" | "submit" | "button" | "image" | "reset") {
            continue;
        }
        let labeled = v.attr("aria-label").is_some()
            || v.attr("aria-labelledby").is_some()
            || v.attr("title").is_some()
            || v.attr("id")
                .is_some_and(|id| labeled_ids.iter().any(|l| l == id))
            || input
                .ancestors()
                .any(|a| a.value().as_element().is_some_and(|e| e.name() == "label"));
        if !labeled {
            doc.unlabeled_inputs += 1;
        }
    }
    for e in html.select(&sel("[style]")) {
        let style = e.value().attr("style").unwrap_or("");
        let color = css_prop(style, "color");
        let bg = css_prop(style, "background-color").or_else(|| css_prop(style, "background"));
        if let Some(color) = color {
            doc.colored.push(HtmlColorStyle {
                text: text_of(e).chars().take(80).collect(),
                color,
                background: bg,
            });
        }
    }
    let body = html.select(&sel("body")).next();
    doc.text = match body {
        Some(b) => b
            .text()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        None => String::new(),
    };
    doc
}

/// Value of a CSS property in an inline style attribute.
fn css_prop(style: &str, prop: &str) -> Option<String> {
    style.split(';').find_map(|decl| {
        let (k, v) = decl.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(prop)).then(|| v.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_structure() {
        let d = parse_html(
            r#"<html lang="en"><head><title>Flu clinic</title></head><body>
            <h1>Flu clinic</h1><h3>Where</h3><img src="a.png"><img src="b.png" alt="">
            <table><tr><td>a</td></tr></table>
            <a href="/x">click here</a><p style="color:#999999;background-color:#ffffff">Grey</p>
            <input id="q"><label for="n">Name</label><input id="n"></body></html>"#,
        );
        assert_eq!(d.language.as_deref(), Some("en"));
        assert_eq!(d.title.as_deref(), Some("Flu clinic"));
        assert_eq!(
            d.headings,
            vec![(1, "Flu clinic".into()), (3, "Where".into())]
        );
        assert_eq!(d.images[0].alt, None);
        assert_eq!(d.images[1].alt.as_deref(), Some(""));
        assert!(!d.tables[0].has_th);
        assert_eq!(d.link_texts, vec!["click here"]);
        assert_eq!(d.colored[0].color, "#999999");
        assert_eq!(d.unlabeled_inputs, 1);
        assert!(d.text.contains("Where"));
    }
}
