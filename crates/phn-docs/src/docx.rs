//! DOCX writer.
//!
//! Hand-written WordprocessingML so every accessibility property is under our
//! control: document title and language, built-in heading styles with outline
//! levels, repeating table header rows with captions, alt text on images,
//! real list numbering, and a letterhead header with page-numbered footer.

use std::io::{Cursor, Write};

use phn_core::{Letterhead, PhnError, Result};
use zip::write::SimpleFileOptions;

use crate::image::{self, ImageFormat};
use crate::model::{Block, Document, Inline, TableBlock};

const W_NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#;
const EMU_PER_PX: u64 = 9525;
const MAX_WIDTH_EMU: u64 = 5_943_600; // 6.5 inches

/// Escape XML text.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            // Strip control characters that are invalid in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => o.push(c),
        }
    }
    o
}

struct Ctx {
    rels: Vec<String>,
    media: Vec<(String, Vec<u8>)>,
    next_rel: u32,
    next_drawing: u32,
    next_num: u32,
    nums: Vec<String>,
}

impl Ctx {
    fn rel(&mut self, kind: &str, target: &str, external: bool) -> String {
        let id = format!("rId{}", self.next_rel);
        self.next_rel += 1;
        self.rels.push(format!(
            r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{}"{}/>"#,
            esc(target),
            if external { r#" TargetMode="External""# } else { "" }
        ));
        id
    }
}

fn run(text: &str, bold: bool, italic: bool, style: Option<&str>) -> String {
    let mut rpr = String::new();
    if let Some(s) = style {
        rpr.push_str(&format!(r#"<w:rStyle w:val="{s}"/>"#));
    }
    if bold {
        rpr.push_str("<w:b/>");
    }
    if italic {
        rpr.push_str("<w:i/>");
    }
    let mut out = String::from("<w:r>");
    if !rpr.is_empty() {
        out.push_str(&format!("<w:rPr>{rpr}</w:rPr>"));
    }
    for (i, part) in text.split('\n').enumerate() {
        if i > 0 {
            out.push_str("<w:br/>");
        }
        out.push_str(&format!(r#"<w:t xml:space="preserve">{}</w:t>"#, esc(part)));
    }
    out.push_str("</w:r>");
    out
}

fn runs(ctx: &mut Ctx, inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        match &i.link {
            Some(url) if url.starts_with("http") || url.starts_with("mailto:") => {
                let id = ctx.rel("hyperlink", url, true);
                out.push_str(&format!(
                    r#"<w:hyperlink r:id="{id}">{}</w:hyperlink>"#,
                    run(&i.text, i.bold, i.italic, Some("Hyperlink"))
                ));
            }
            _ => out.push_str(&run(&i.text, i.bold, i.italic, None)),
        }
    }
    out
}

fn para(style: Option<&str>, content: &str) -> String {
    match style {
        Some(s) => format!(r#"<w:p><w:pPr><w:pStyle w:val="{s}"/></w:pPr>{content}</w:p>"#),
        None => format!("<w:p>{content}</w:p>"),
    }
}

fn table_xml(t: &TableBlock, number: usize) -> String {
    let cols = t
        .headers
        .len()
        .max(t.rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let width = 9360 / cols; // twips across 6.5 inches
    let mut x = String::new();
    let caption = t
        .caption
        .clone()
        .unwrap_or_else(|| format!("Table {number}"));
    x.push_str(&para(
        Some("Caption"),
        &run(&format!("Table {number}. {caption}"), false, false, None),
    ));
    x.push_str(&format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="5000" w:type="pct"/><w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/><w:tblCaption w:val="{}"/></w:tblPr><w:tblGrid>"#,
        esc(&caption)
    ));
    for _ in 0..cols {
        x.push_str(&format!(r#"<w:gridCol w:w="{width}"/>"#));
    }
    x.push_str("</w:tblGrid>");
    let cell = |text: &str, header: bool| {
        format!(
            r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{}</w:tcPr><w:p>{}</w:p></w:tc>"#,
            if header {
                r#"<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/>"#
            } else {
                ""
            },
            run(text, header, false, None)
        )
    };
    if !t.headers.is_empty() {
        x.push_str(r#"<w:tr><w:trPr><w:tblHeader/><w:cantSplit/></w:trPr>"#);
        for c in 0..cols {
            x.push_str(&cell(
                t.headers.get(c).map(String::as_str).unwrap_or(""),
                true,
            ));
        }
        x.push_str("</w:tr>");
    }
    for r in &t.rows {
        x.push_str("<w:tr><w:trPr><w:cantSplit/></w:trPr>");
        for c in 0..cols {
            x.push_str(&cell(r.get(c).map(String::as_str).unwrap_or(""), false));
        }
        x.push_str("</w:tr>");
    }
    x.push_str("</w:tbl>");
    // Word needs a paragraph between consecutive tables.
    x.push_str("<w:p/>");
    x
}

fn image_xml(
    ctx: &mut Ctx,
    data: &[u8],
    alt: &str,
    max_width_emu: u64,
    max_height_emu: Option<u64>,
) -> Option<String> {
    let info = image::inspect(data)?;
    let ext = match info.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
    };
    let n = ctx.media.len() + 1;
    let name = format!("image{n}.{ext}");
    ctx.media.push((name.clone(), data.to_vec()));
    let rid = ctx.rel("image", &format!("media/{name}"), false);
    let mut cx = u64::from(info.width.max(1)) * EMU_PER_PX;
    let mut cy = u64::from(info.height.max(1)) * EMU_PER_PX;
    if cx > max_width_emu {
        cy = cy * max_width_emu / cx;
        cx = max_width_emu;
    }
    if let Some(mh) = max_height_emu
        && cy > mh
    {
        cx = cx * mh / cy;
        cy = mh;
    }
    ctx.next_drawing += 1;
    let id = ctx.next_drawing;
    let alt = esc(alt);
    Some(format!(
        r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="Picture {id}" descr="{alt}"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="{name}" descr="{alt}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    ))
}

fn list_num(ctx: &mut Ctx, ordered: bool) -> u32 {
    ctx.next_num += 1;
    let id = ctx.next_num;
    let abs = if ordered { 1 } else { 0 };
    ctx.nums.push(format!(
        r#"<w:num w:numId="{id}"><w:abstractNumId w:val="{abs}"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>"#
    ));
    id
}

fn body(ctx: &mut Ctx, doc: &Document) -> String {
    let mut x = String::new();
    x.push_str(&para(Some("Title"), &run(&doc.title, false, false, None)));
    if let Some(s) = &doc.subtitle {
        x.push_str(&para(Some("Subtitle"), &run(s, false, false, None)));
    }
    let mut table_no = 0;
    for b in &doc.blocks {
        match b {
            Block::Heading { level, text } => {
                let l = (*level).clamp(1, 6);
                x.push_str(&para(
                    Some(&format!("Heading{l}")),
                    &run(text, false, false, None),
                ));
            }
            Block::Paragraph(r) => {
                let content = runs(ctx, r);
                x.push_str(&para(None, &content));
            }
            Block::List { ordered, items } => {
                let num = list_num(ctx, *ordered);
                for item in items {
                    let content = runs(ctx, item);
                    x.push_str(&format!(
                        r#"<w:p><w:pPr><w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="{num}"/></w:numPr></w:pPr>{content}</w:p>"#
                    ));
                }
            }
            Block::Table(t) => {
                table_no += 1;
                x.push_str(&table_xml(t, table_no));
            }
            Block::Image(img) => {
                if let Some(data) = &img.data
                    && let Some(drawing) = image_xml(ctx, data, &img.alt, MAX_WIDTH_EMU, None)
                {
                    x.push_str(&format!("<w:p>{drawing}</w:p>"));
                    if !img.alt.is_empty() {
                        x.push_str(&para(Some("Caption"), &run(&img.alt, false, false, None)));
                    }
                }
            }
            Block::PageBreak => x.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#),
        }
    }
    x
}

fn contact_line(l: &Letterhead) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = &l.phone {
        parts.push(format!("Phone {p}"));
    }
    if let Some(f) = &l.fax {
        parts.push(format!("Fax {f}"));
    }
    if let Some(e) = &l.email {
        parts.push(e.clone());
    }
    if let Some(w) = &l.website {
        parts.push(w.clone());
    }
    parts.join(" · ")
}

/// Lines of the letterhead: (text, bold).
pub fn letterhead_lines(l: &Letterhead) -> Vec<(String, bool)> {
    let mut v = vec![(l.agency_name.clone(), true)];
    if let Some(d) = &l.department {
        v.push((d.clone(), false));
    }
    if !l.address_lines.is_empty() {
        v.push((l.address_lines.join(", "), false));
    }
    let c = contact_line(l);
    if !c.is_empty() {
        v.push((c, false));
    }
    v
}

fn styles_xml(lang: &str) -> String {
    let heading = |id: u8, size: u32, before: u32| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{id}"><w:name w:val="heading {id}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:uiPriority w:val="9"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="{before}" w:after="80"/><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:b/><w:color w:val="1F3864"/><w:sz w:val="{size}"/></w:rPr></w:style>"#,
            id - 1
        )
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles {W_NS}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:lang w:val="{lang}" w:eastAsia="{lang}" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="120"/></w:pPr><w:rPr><w:b/><w:color w:val="1F3864"/><w:sz w:val="40"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:color w:val="404040"/><w:sz w:val="26"/></w:rPr></w:style>
{}{}{}{}{}{}
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="60"/><w:ind w:left="720"/><w:contextualSpacing/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Caption"><w:name w:val="caption"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="160" w:after="60"/></w:pPr><w:rPr><w:b/><w:color w:val="404040"/><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Header"><w:name w:val="header"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="18"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Footer"><w:name w:val="footer"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:color w:val="404040"/><w:sz w:val="18"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0B5394"/><w:u w:val="single"/></w:rPr></w:style>
<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:left w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:right w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="808080"/></w:tblBorders><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
</w:styles>"#,
        heading(1, 32, 360),
        heading(2, 28, 240),
        heading(3, 24, 200),
        heading(4, 22, 160),
        heading(5, 22, 160),
        heading(6, 22, 160),
    )
}

const NUMBERING: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="singleLevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="singleLevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>{NUMS}</w:numbering>"#;

/// Render a document as DOCX bytes.
pub fn to_docx(doc: &Document) -> Result<Vec<u8>> {
    let mut ctx = Ctx {
        rels: Vec::new(),
        media: Vec::new(),
        next_rel: 10,
        next_drawing: 0,
        next_num: 0,
        nums: Vec::new(),
    };
    let lang = esc(&doc.language);
    let body_xml = body(&mut ctx, doc);

    // Header (letterhead) and footer.
    let mut header_rels: Vec<String> = Vec::new();
    let mut header_media: Vec<(String, Vec<u8>)> = Vec::new();
    let header_xml = doc.letterhead.as_ref().map(|l| {
        let mut x = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {W_NS}>"#);
        if let Some(logo) = &doc.logo {
            // The logo lives in the header part, with its own relationships.
            let mut hctx = Ctx { rels: Vec::new(), media: Vec::new(), next_rel: 1, next_drawing: 900, next_num: 0, nums: Vec::new() };
            if let Some(d) = image_xml(&mut hctx, logo, &format!("{} logo", l.agency_name), 1_828_800, Some(640_080)) {
                x.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="Header"/></w:pPr>{d}</w:p>"#));
                header_rels = hctx.rels;
                header_media = hctx.media.into_iter().map(|(n, b)| (format!("hdr_{n}"), b)).collect();
                for r in header_rels.iter_mut() {
                    *r = r.replace("media/image", "media/hdr_image");
                }
            }
        }
        for (text, bold) in letterhead_lines(l) {
            x.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="Header"/></w:pPr>{}</w:p>"#, run(&text, bold, false, None)));
        }
        x.push_str(r#"<w:p><w:pPr><w:pStyle w:val="Header"/><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="1F3864"/></w:pBdr></w:pPr></w:p></w:hdr>"#);
        x
    });
    let footer_text = doc
        .letterhead
        .as_ref()
        .and_then(|l| l.footer_text.clone())
        .map(|t| format!("{} · ", esc(&t)))
        .unwrap_or_default();
    let footer_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr {W_NS}><w:p><w:pPr><w:pStyle w:val="Footer"/><w:jc w:val="center"/></w:pPr><w:r><w:t xml:space="preserve">{footer_text}Page </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t xml:space="preserve"> of </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:ftr>"#
    );
    let header_rid = header_xml
        .as_ref()
        .map(|_| ctx.rel("header", "header1.xml", false));
    let footer_rid = ctx.rel("footer", "footer1.xml", false);

    let sect = format!(
        r#"<w:sectPr>{}<w:footerReference w:type="default" r:id="{footer_rid}"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#,
        header_rid
            .as_ref()
            .map(|id| format!(r#"<w:headerReference w:type="default" r:id="{id}"/>"#))
            .unwrap_or_default()
    );
    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{body_xml}{sect}</w:body></w:document>"#
    );

    let mut doc_rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
    );
    for r in &ctx.rels {
        doc_rels.push_str(r);
    }
    doc_rels.push_str("</Relationships>");

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let author = esc(doc
        .author
        .as_deref()
        .or(doc.letterhead.as_ref().map(|l| l.agency_name.as_str()))
        .unwrap_or("Public Health Nursing MCP Server"));
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>{author}</dc:creator><dc:language>{lang}</dc:language><dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{now}</dcterms:modified></cp:coreProperties>"#,
        esc(&doc.title)
    );
    let app = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Public Health Nursing MCP Server</Application></Properties>"#;
    let settings = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:updateFields w:val="true"/><w:defaultTabStop w:val="720"/><w:themeFontLang w:val="{lang}"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#
    );
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>{}<Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#,
        if header_xml.is_some() {
            r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#
        } else {
            ""
        }
    );
    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;

    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let zerr = |e: zip::result::ZipError| PhnError::Internal(format!("DOCX zip: {e}"));
        let mut put = |name: &str, data: &[u8]| -> Result<()> {
            z.start_file(name, o).map_err(zerr)?;
            z.write_all(data)?;
            Ok(())
        };
        put("[Content_Types].xml", content_types.as_bytes())?;
        put("_rels/.rels", root_rels.as_bytes())?;
        put("docProps/core.xml", core.as_bytes())?;
        put("docProps/app.xml", app.as_bytes())?;
        put("word/document.xml", document_xml.as_bytes())?;
        put("word/_rels/document.xml.rels", doc_rels.as_bytes())?;
        put("word/styles.xml", styles_xml(&lang).as_bytes())?;
        put("word/settings.xml", settings.as_bytes())?;
        put(
            "word/numbering.xml",
            NUMBERING.replace("{NUMS}", &ctx.nums.concat()).as_bytes(),
        )?;
        put("word/footer1.xml", footer_xml.as_bytes())?;
        if let Some(h) = &header_xml {
            put("word/header1.xml", h.as_bytes())?;
            let rels = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{}</Relationships>"#,
                header_rels.concat()
            );
            put("word/_rels/header1.xml.rels", rels.as_bytes())?;
        }
        for (name, data) in ctx.media.iter().chain(header_media.iter()) {
            put(&format!("word/media/{name}"), data)?;
        }
        z.finish().map_err(zerr)?;
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use crate::model::from_markdown;

    /// A representative document used by several tests.
    pub(crate) fn sample() -> Document {
        Document {
            title: "Pertussis update".into(),
            subtitle: Some("For school nurses".into()),
            language: "en-US".into(),
            author: None,
            letterhead: Some(Letterhead {
                agency_name: "Town of Example Health Department".into(),
                address_lines: vec!["1 Main St".into(), "Example, MA 01000".into()],
                phone: Some("413-555-0100".into()),
                footer_text: Some("Confidential".into()),
                ..Letterhead::default()
            }),
            logo: None,
            blocks: from_markdown(
                "# Situation\n\nCases are **rising** & we need help <now>.\n\n## Actions\n\n- Exclude cases\n- Notify families\n\nTable: Cases\n\n| Week | Cases |\n|---|---|\n| 1 | 3 |\n",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::sample as sample_doc;

    #[test]
    fn writes_a_docx_that_reads_back_with_structure() {
        let bytes = to_docx(&sample_doc()).unwrap();
        let parsed = phn_local::docx::parse_docx(&bytes).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Pertussis update"));
        assert_eq!(parsed.language.as_deref(), Some("en-US"));
        let headings: Vec<_> = parsed
            .paragraphs()
            .filter_map(|p| p.heading_level.map(|l| (l, p.text.clone())))
            .collect();
        assert_eq!(
            headings,
            vec![(1, "Situation".into()), (2, "Actions".into())]
        );
        let t = parsed.tables().next().unwrap();
        assert!(t.header_row_marked);
        assert_eq!(t.rows[1], vec!["1", "3"]);
        assert!(parsed.plain_text().contains("we need help <now>"));
    }

    #[test]
    fn embeds_images_with_alt_text() {
        let mut pm = resvg::tiny_skia::Pixmap::new(40, 20).unwrap();
        pm.fill(resvg::tiny_skia::Color::BLACK);
        let png = pm.encode_png().unwrap();
        let mut d = sample_doc();
        d.logo = Some(png.clone());
        d.blocks.push(Block::Image(crate::model::ImageBlock {
            path: "x.png".into(),
            alt: "Bar chart of cases by week".into(),
            data: Some(png),
        }));
        let bytes = to_docx(&d).unwrap();
        let parsed = phn_local::docx::parse_docx(&bytes).unwrap();
        assert_eq!(parsed.drawings.len(), 1);
        assert_eq!(
            parsed.drawings[0].alt_text.as_deref(),
            Some("Bar chart of cases by week")
        );
        let mut z = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        assert!(z.by_name("word/media/hdr_image1.png").is_ok());
        assert!(z.by_name("word/media/image1.png").is_ok());
    }
}
