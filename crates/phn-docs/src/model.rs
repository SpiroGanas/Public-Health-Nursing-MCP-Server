//! Document model and Markdown conversion.

use phn_core::Letterhead;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

/// A run of text.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Inline {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub link: Option<String>,
}

impl Inline {
    /// Plain text run.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

/// A table.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TableBlock {
    pub caption: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// An image to embed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageBlock {
    /// Path relative to /data (as written in the Markdown).
    pub path: String,
    pub alt: String,
    /// Image bytes (PNG or JPEG), filled in by the caller.
    #[serde(skip)]
    pub data: Option<Vec<u8>>,
}

/// A block-level element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(Vec<Inline>),
    List {
        ordered: bool,
        items: Vec<Vec<Inline>>,
    },
    Table(TableBlock),
    Image(ImageBlock),
    PageBreak,
}

/// A document ready to export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub subtitle: Option<String>,
    /// BCP 47 language tag, e.g. "en-US" or "es-US".
    pub language: String,
    pub author: Option<String>,
    pub letterhead: Option<Letterhead>,
    /// Logo bytes for the letterhead, filled in by the caller.
    #[serde(skip)]
    pub logo: Option<Vec<u8>>,
    pub blocks: Vec<Block>,
}

impl Document {
    /// Tables in the document.
    pub fn tables(&self) -> Vec<&TableBlock> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Table(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// All text in the document (used for placeholder discovery).
    pub fn all_text(&self) -> String {
        let mut s = format!(
            "{}\n{}\n",
            self.title,
            self.subtitle.clone().unwrap_or_default()
        );
        for b in &self.blocks {
            match b {
                Block::Heading { text, .. } => s.push_str(text),
                Block::Paragraph(r) => r.iter().for_each(|i| s.push_str(&i.text)),
                Block::List { items, .. } => {
                    items.iter().flatten().for_each(|i| s.push_str(&i.text))
                }
                Block::Table(t) => {
                    s.push_str(&t.headers.join(" "));
                    t.rows.iter().flatten().for_each(|c| s.push_str(c));
                    if let Some(c) = &t.caption {
                        s.push_str(c);
                    }
                }
                Block::Image(i) => s.push_str(&i.alt),
                Block::PageBreak => {}
            }
            s.push('\n');
        }
        s
    }

    /// Apply a function to every piece of text (used by mail-merge).
    pub fn map_text(&self, f: &dyn Fn(&str) -> String) -> Document {
        let mut d = self.clone();
        d.title = f(&d.title);
        d.subtitle = d.subtitle.as_deref().map(f);
        for b in &mut d.blocks {
            match b {
                Block::Heading { text, .. } => *text = f(text),
                Block::Paragraph(runs) => runs.iter_mut().for_each(|r| r.text = f(&r.text)),
                Block::List { items, .. } => {
                    items.iter_mut().flatten().for_each(|r| r.text = f(&r.text))
                }
                Block::Table(t) => {
                    t.caption = t.caption.as_deref().map(f);
                    t.headers.iter_mut().for_each(|h| *h = f(h));
                    t.rows.iter_mut().flatten().for_each(|c| *c = f(c));
                }
                Block::Image(i) => i.alt = f(&i.alt),
                Block::PageBreak => {}
            }
        }
        d
    }
}

/// Convert Markdown (CommonMark with tables) to blocks.
///
/// A line containing only `\pagebreak` or `<!-- pagebreak -->` starts a new page;
/// a horizontal rule (`---`) does not. Nested list items stay inside their
/// parent item, each on its own line.
pub fn from_markdown(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    let mut runs: Vec<Inline> = Vec::new();
    let mut bold = 0u32;
    let mut italic = 0u32;
    let mut link: Option<String> = None;
    let mut heading: Option<u8> = None;
    let mut list_stack: Vec<(bool, Vec<Vec<Inline>>)> = Vec::new();
    // Text of the list items that are open around a nested list.
    let mut parent_items: Vec<Vec<Inline>> = Vec::new();
    let mut in_item = false;
    let mut table: Option<TableBlock> = None;
    let mut in_head = false;
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_cell = false;
    let mut image: Option<(String, String)> = None;

    let push_text =
        |runs: &mut Vec<Inline>, t: &str, bold: u32, italic: u32, link: &Option<String>| {
            if let Some(last) = runs.last_mut()
                && last.bold == (bold > 0)
                && last.italic == (italic > 0)
                && last.link == *link
            {
                last.text.push_str(t);
                return;
            }
            runs.push(Inline {
                text: t.to_string(),
                bold: bold > 0,
                italic: italic > 0,
                link: link.clone(),
            });
        };

    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                });
                runs.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                let text: String = runs.drain(..).map(|r| r.text).collect();
                blocks.push(Block::Heading {
                    level: heading.take().unwrap_or(1),
                    text: text.trim().to_string(),
                });
            }
            Event::Start(Tag::Paragraph) => {
                if !in_item {
                    runs.clear();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !in_item && table.is_none() {
                    let r: Vec<Inline> = std::mem::take(&mut runs);
                    let text: String = r.iter().map(|i| i.text.as_str()).collect();
                    if text.trim() == "\\pagebreak" {
                        blocks.push(Block::PageBreak);
                    } else if !text.trim().is_empty() {
                        blocks.push(Block::Paragraph(r));
                    }
                } else if in_item {
                    push_text(&mut runs, " ", bold, italic, &link);
                }
            }
            Event::Start(Tag::List(start)) => {
                if !list_stack.is_empty() {
                    // Nested list: set the open parent item aside; the nested
                    // items are added to it when the nested list ends.
                    parent_items.push(std::mem::take(&mut runs));
                }
                list_stack.push((start.is_some(), Vec::new()));
                in_item = false;
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((ordered, items)) = list_stack.pop() {
                    if list_stack.is_empty() {
                        blocks.push(Block::List { ordered, items });
                    } else {
                        // Keep nested items inside their parent item, in
                        // reading order, each on its own line.
                        let mut parent = parent_items.pop().unwrap_or_default();
                        if let Some(last) = parent.last_mut() {
                            last.text = last.text.trim_end().to_string();
                        }
                        for (n, item) in items.into_iter().enumerate() {
                            let mark = if ordered {
                                format!("{}. ", n + 1)
                            } else {
                                "– ".to_string()
                            };
                            let lead = if parent.is_empty() { "" } else { "\n" };
                            parent.push(Inline::plain(format!("{lead}{mark}")));
                            parent.extend(item);
                        }
                        runs = parent;
                    }
                }
                in_item = !list_stack.is_empty();
            }
            Event::Start(Tag::Item) => {
                runs.clear();
                in_item = true;
            }
            Event::End(TagEnd::Item) => {
                let mut item: Vec<Inline> = std::mem::take(&mut runs);
                if let Some(last) = item.last_mut() {
                    last.text = last.text.trim_end().to_string();
                }
                if !item.iter().all(|i| i.text.trim().is_empty())
                    && let Some((_, items)) = list_stack.last_mut()
                {
                    items.push(item);
                }
            }
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => link = None,
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((dest_url.to_string(), String::new()))
            }
            Event::End(TagEnd::Image) => {
                if let Some((path, alt)) = image.take() {
                    blocks.push(Block::Image(ImageBlock {
                        path,
                        alt: alt.trim().to_string(),
                        data: None,
                    }));
                }
            }
            Event::Start(Tag::Table(_)) => {
                table = Some(TableBlock::default());
                // A paragraph immediately before a table that starts with
                // "Table:" becomes its caption.
                if let Some(Block::Paragraph(r)) = blocks.last() {
                    let text: String = r.iter().map(|i| i.text.as_str()).collect();
                    if let Some(c) = text.trim().strip_prefix("Table:") {
                        let caption = c.trim().to_string();
                        blocks.pop();
                        if let Some(t) = table.as_mut() {
                            t.caption = Some(caption);
                        }
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take() {
                    blocks.push(Block::Table(t));
                }
            }
            Event::Start(Tag::TableHead) => {
                in_head = true;
                row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                in_head = false;
                if let Some(t) = table.as_mut() {
                    t.headers = std::mem::take(&mut row);
                }
            }
            Event::Start(Tag::TableRow) => row.clear(),
            Event::End(TagEnd::TableRow) => {
                if !in_head && let Some(t) = table.as_mut() {
                    t.rows.push(std::mem::take(&mut row));
                }
            }
            Event::Start(Tag::TableCell) => {
                in_cell = true;
                cell.clear();
            }
            Event::End(TagEnd::TableCell) => {
                in_cell = false;
                row.push(cell.trim().to_string());
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, alt)) = image.as_mut() {
                    alt.push_str(&t);
                } else if in_cell {
                    cell.push_str(&t);
                } else {
                    push_text(&mut runs, &t, bold, italic, &link);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_cell {
                    cell.push(' ');
                } else {
                    push_text(&mut runs, " ", bold, italic, &link);
                }
            }
            Event::Start(Tag::CodeBlock(_)) | Event::Start(Tag::HtmlBlock) => {
                if !in_item {
                    runs.clear();
                }
            }
            Event::End(TagEnd::CodeBlock) | Event::End(TagEnd::HtmlBlock) => {
                // Indented or fenced text (an address block, a script) and
                // block-level HTML are kept as plain paragraphs, one per line.
                if in_item {
                    push_text(&mut runs, " ", bold, italic, &link);
                } else if table.is_none() {
                    let text: String = std::mem::take(&mut runs)
                        .into_iter()
                        .map(|r| r.text)
                        .collect();
                    for line in text.lines().filter(|l| !l.trim().is_empty()) {
                        blocks.push(Block::Paragraph(vec![Inline::plain(line.trim_end())]));
                    }
                }
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                // Only the exact comment starts a new page.
                let is_page_break = h
                    .trim()
                    .strip_prefix("<!--")
                    .and_then(|s| s.strip_suffix("-->"))
                    .is_some_and(|s| s.trim().eq_ignore_ascii_case("pagebreak"));
                if is_page_break {
                    blocks.push(Block::PageBreak);
                } else if !h.trim_start().starts_with("<!--") {
                    // Keep angle-bracket text rather than silently dropping it.
                    if in_cell {
                        cell.push_str(&h);
                    } else {
                        push_text(&mut runs, &h, bold, italic, &link);
                    }
                }
            }
            // A horizontal rule ("---") is dropped: it does not start a new page.
            Event::Rule => {}
            _ => {}
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_markdown() {
        let md = "# Report\n\nSome **bold** and [a link](https://mass.gov).\n\n- one\n- two\n\n1. a\n2. b\n\nTable: Cases by town\n\n| Town | Cases |\n|---|---|\n| Amherst | 3 |\n\n![Map of cases](maps/cases.png)\n\n\\pagebreak\n\nEnd.";
        let b = from_markdown(md);
        assert_eq!(
            b[0],
            Block::Heading {
                level: 1,
                text: "Report".into()
            }
        );
        match &b[1] {
            Block::Paragraph(r) => {
                assert!(r.iter().any(|i| i.bold && i.text == "bold"));
                assert!(
                    r.iter()
                        .any(|i| i.link.as_deref() == Some("https://mass.gov"))
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&b[2], Block::List { ordered: false, items } if items.len() == 2));
        assert!(matches!(&b[3], Block::List { ordered: true, .. }));
        match &b[4] {
            Block::Table(t) => {
                assert_eq!(t.caption.as_deref(), Some("Cases by town"));
                assert_eq!(t.headers, vec!["Town", "Cases"]);
                assert_eq!(t.rows, vec![vec!["Amherst", "3"]]);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&b[5], Block::Image(i) if i.alt == "Map of cases"));
        assert_eq!(b[6], Block::PageBreak);
    }

    fn item_text(item: &[Inline]) -> String {
        item.iter().map(|i| i.text.as_str()).collect()
    }

    #[test]
    fn nested_list_items_stay_in_order_inside_their_parent() {
        let b = from_markdown(
            "1. Exclude the case\n   - until 5 days of antibiotics\n   - or 21 days from cough onset\n2. Notify families\n",
        );
        assert_eq!(b.len(), 1, "{b:?}");
        match &b[0] {
            Block::List { ordered, items } => {
                assert!(*ordered);
                assert_eq!(items.len(), 2);
                assert_eq!(
                    item_text(&items[0]),
                    "Exclude the case\n– until 5 days of antibiotics\n– or 21 days from cough onset"
                );
                assert_eq!(item_text(&items[1]), "Notify families");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn indented_and_fenced_text_is_kept() {
        let b = from_markdown(
            "Send it to:\n\n    Board of Health\n    1 Main St\n\n```\nSay this aloud.\n```\n\nThanks.",
        );
        let text: Vec<String> = b
            .iter()
            .filter_map(|x| match x {
                Block::Paragraph(r) => Some(item_text(r)),
                _ => None,
            })
            .collect();
        assert_eq!(
            text,
            vec![
                "Send it to:",
                "Board of Health",
                "1 Main St",
                "Say this aloud.",
                "Thanks."
            ]
        );
    }

    #[test]
    fn only_an_explicit_marker_breaks_the_page() {
        let b = from_markdown(
            "One.\n\n---\n\nTwo.\n\n<!-- pagebreak -->\n\nThree.\n\n<!-- no pagebreak here -->\n\nFour.",
        );
        let breaks = b.iter().filter(|x| **x == Block::PageBreak).count();
        assert_eq!(breaks, 1, "{b:?}");
        assert_eq!(b.len(), 5);
        assert_eq!(b[2], Block::PageBreak);
    }
}
