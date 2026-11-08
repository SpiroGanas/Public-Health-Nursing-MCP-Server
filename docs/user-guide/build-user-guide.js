#!/usr/bin/env node
// Builds docs/user-guide/PHN-MCP-Server-User-Guide.docx from guide-content.js.
//
//   node docs/user-guide/build-user-guide.js [output.docx]
//
// Needs the `docx` npm package and, for the table of contents page numbers, LibreOffice
// (`soffice`) and Poppler (`pdftotext`). The document is laid out twice: the first pass is
// converted to PDF to find the page of each heading, and the second pass writes those page
// numbers into the table of contents. The contents are wrapped in a real Word TOC field, so
// Word can also refresh them (right-click > Update Field). Without LibreOffice the guide is
// still written, with the page numbers left blank.

'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');
const docx = require('docx');
const JSZip = require(require.resolve('jszip', { paths: [path.dirname(require.resolve('docx'))] }));
const C = require('./guide-content');

const {
  AlignmentType, BorderStyle, Bookmark, Document, Footer, InternalHyperlink, LevelFormat,
  Packer, PageNumber, Paragraph, ShadingType, Table, TableCell, TableRow, TabStopType, TextRun,
  WidthType, HeadingLevel, TableLayoutType, LineRuleType, Tab, LeaderType,
} = docx;

// ---------------------------------------------------------------------------------------
// Design tokens
// ---------------------------------------------------------------------------------------

const FONT = 'Arial';
const MONO = 'Courier New';
const NAVY = '1F3A5F';
const ACCENT = '2E5C8A';
const RULE = '2E75B6';
const TEXT = '1A1A1A';
const MUTED = '4D4D4D';
const LIGHT_FILL = 'EEF3F8';
const FACT_FILL = 'F2F4F7';
const GRID = 'B7C3D0';
const PAGE_W = 12240;
const MARGIN = 1440;
const CONTENT_W = PAGE_W - 2 * MARGIN; // 9360

// ---------------------------------------------------------------------------------------
// Inline markup: **bold**, *italic*, `code`
// ---------------------------------------------------------------------------------------

function runs(text, base = {}) {
  const out = [];
  const re = /(\*\*[^*]+\*\*|\*[^*\s][^*]*\*|`[^`]+`)/g;
  let last = 0;
  let m;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push(new TextRun({ text: text.slice(last, m.index), ...base }));
    const tok = m[0];
    if (tok.startsWith('**')) out.push(new TextRun({ text: tok.slice(2, -2), ...base, bold: true }));
    else if (tok.startsWith('`')) out.push(new TextRun({ text: tok.slice(1, -1), ...base, font: MONO }));
    else out.push(new TextRun({ text: tok.slice(1, -1), ...base, italics: true }));
    last = m.index + tok.length;
  }
  if (last < text.length) out.push(new TextRun({ text: text.slice(last), ...base }));
  return out;
}

const plain = (s) => s.replace(/\*\*|`/g, '').replace(/\*([^*]+)\*/g, '$1');

// ---------------------------------------------------------------------------------------
// Block builders
// ---------------------------------------------------------------------------------------

let stepsInstance = 0;

const para = (text, opts = {}) => new Paragraph({ children: runs(text), ...opts });
const bullets = (items) =>
  items.map((t) => new Paragraph({ children: runs(t), numbering: { reference: 'bullets', level: 0 }, style: 'ListText' }));
const steps = (items) => {
  stepsInstance += 1;
  const instance = stepsInstance;
  return items.map(
    (t) => new Paragraph({ children: runs(t), numbering: { reference: 'steps', level: 0, instance }, style: 'ListText' }),
  );
};
const example = (t) => new Paragraph({ style: 'Example', children: runs(`“${t}”`) });
const note = (t) => new Paragraph({ style: 'Note', children: runs(t) });

function cell(content, width, { header = false, fill } = {}) {
  const paragraphs = (Array.isArray(content) ? content : [content]).map((c) =>
    c instanceof Paragraph
      ? c
      : new Paragraph({
          style: 'TableText',
          children: runs(String(c), header ? { bold: true, color: 'FFFFFF' } : {}),
        }),
  );
  return new TableCell({
    width: { size: width, type: WidthType.DXA },
    shading: header ? { type: ShadingType.CLEAR, color: 'auto', fill: NAVY } : fill ? { type: ShadingType.CLEAR, color: 'auto', fill } : undefined,
    margins: { top: 60, bottom: 60, left: 100, right: 100 },
    children: paragraphs,
  });
}

function table({ caption, widths, header, rows }) {
  const total = widths.reduce((a, b) => a + b, 0);
  if (total !== CONTENT_W) throw new Error(`table widths sum to ${total}, not ${CONTENT_W}: ${caption}`);
  const border = { style: BorderStyle.SINGLE, size: 4, color: GRID };
  const out = [];
  if (caption) out.push(new Paragraph({ style: 'Caption', children: runs(caption) }));
  out.push(
    new Table({
      width: { size: CONTENT_W, type: WidthType.DXA },
      columnWidths: widths,
      layout: TableLayoutType.FIXED,
      borders: { top: border, bottom: border, left: border, right: border, insideHorizontal: border, insideVertical: border },
      rows: [
        new TableRow({ tableHeader: true, cantSplit: true, children: header.map((h, i) => cell(h, widths[i], { header: true })) }),
        ...rows.map(
          (r, ri) =>
            new TableRow({
              cantSplit: true,
              children: r.map((c, i) => cell(c, widths[i], { fill: ri % 2 === 1 ? LIGHT_FILL : undefined })),
            }),
        ),
      ],
    }),
  );
  // A short spacer paragraph after a table keeps the next block from touching it.
  out.push(new Paragraph({ style: 'AfterTable', children: [] }));
  return out;
}

function blocks(list) {
  const out = [];
  for (const b of list) {
    if (b.p) out.push(para(b.p));
    else if (b.bullets) out.push(...bullets(b.bullets));
    else if (b.steps) out.push(...steps(b.steps));
    else if (b.table) out.push(...table(b.table));
    else if (b.example) out.push(example(b.example));
    else if (b.note) out.push(note(b.note));
    else throw new Error(`unknown block: ${JSON.stringify(b)}`);
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// Headings (collected for the table of contents)
// ---------------------------------------------------------------------------------------

function makeHeadingCollector() {
  const headings = [];
  function heading(level, text, bookmark, { toc = level <= 2, newPage = false } = {}) {
    if (toc) headings.push({ level, text, bookmark });
    const lvl = [null, HeadingLevel.HEADING_1, HeadingLevel.HEADING_2, HeadingLevel.HEADING_3][level];
    const children = bookmark ? [new Bookmark({ id: bookmark, children: [new TextRun(text)] })] : [new TextRun(text)];
    return new Paragraph({ heading: lvl, pageBreakBefore: newPage, children });
  }
  return { headings, heading };
}

// ---------------------------------------------------------------------------------------
// Document body
// ---------------------------------------------------------------------------------------

function toolFacts(t) {
  const label = (s) => new TextRun({ text: s, bold: true });
  const sep = () => new TextRun({ text: '   |   ', color: MUTED });
  return new Paragraph({
    style: 'ToolFacts',
    children: [
      label('Tool name: '),
      new TextRun({ text: t.id, font: MONO }),
      sep(),
      label('Group: '),
      new TextRun(C.TOOLSETS[t.toolset]),
      new TextRun({ break: 1 }),
      label('Internet: '),
      new TextRun(C.INTERNET[t.internet]),
      sep(),
      label('Saves files: '),
      new TextRun(t.files),
    ],
  });
}

function toolSection(t, num, heading) {
  for (const k of ['id', 'title', 'toolset', 'internet', 'files', 'does', 'when', 'provide', 'examples', 'returns', 'notes']) {
    if (t[k] === undefined) throw new Error(`tool ${t.id || t.title} is missing ${k}`);
  }
  if (!C.TOOLSETS[t.toolset]) throw new Error(`unknown toolset ${t.toolset}`);
  return [
    heading(2, `${num} ${t.title}`, `tool_${t.id}`.slice(0, 40)),
    toolFacts(t),
    para(t.does),
    heading(3, 'When to use it'),
    ...bullets(t.when),
    heading(3, 'How to use it'),
    para('Tell Claude as much of the following as applies:'),
    ...bullets(t.provide),
    para('For example:', { keepNext: true }),
    ...t.examples.map(example),
    heading(3, 'What you get back'),
    ...bullets(t.returns),
    heading(3, 'Keep in mind'),
    ...bullets(t.notes),
  ];
}

function titlePage() {
  return [
    new Paragraph({ style: 'Title', children: [new TextRun('Public Health Nursing MCP Server')] }),
    new Paragraph({ style: 'Subtitle', children: [new TextRun('User Guide for Public Health Nurses')] }),
    new Paragraph({ style: 'CoverMeta', children: [new TextRun(`Version ${C.VERSION}  ·  ${C.GUIDE_DATE}`)] }),
    para(
      `A practical guide to the ${C.TOOL_COUNT} tools that help AI assistants such as Claude support public health nursing practice in Massachusetts: communicable disease control, immunizations, epidemiology, community health and equity, and evidence-based practice. For each tool it explains what the tool does, when to use it, and how to ask for it.`,
      { style: 'CoverText' },
    ),
    note('**Decision support only.** The tools support, and never replace, your professional judgment, your agency’s policies and standing orders, and consultation with DPH.'),
    note('**Draft reference content.** The bundled regulatory and clinical content is pending review by the project’s subject matter expert. Every answer built on it says so.'),
    new Paragraph({
      style: 'CoverMeta',
      spacing: { before: 1200 },
      children: [new TextRun('Free and open source (MIT License): github.com/SpiroGanas/Public-Health-Nursing-MCP-Server')],
    }),
  ];
}

function buildBody(heading) {
  const out = [];
  let chapter = 0;

  for (const ch of C.frontChapters) {
    chapter += 1;
    out.push(heading(1, `${chapter}. ${ch.title}`, `sec_${chapter}`, { newPage: true }));
    out.push(...blocks(ch.intro));
    ch.sections.forEach((s, i) => {
      out.push(heading(2, `${chapter}.${i + 1} ${s.title}`, `sec_${chapter}_${i + 1}`));
      out.push(...blocks(s.blocks));
    });
  }

  const index = [];
  for (const ch of C.toolChapters) {
    chapter += 1;
    out.push(heading(1, `${chapter}. ${ch.title}`, `sec_${chapter}`, { newPage: true }));
    out.push(para(ch.intro));
    let n = 0;
    for (const t of ch.tools) {
      n += 1;
      const num = `${chapter}.${n}`;
      out.push(...toolSection(t, num, heading));
      index.push({ num, t });
    }
    for (const x of ch.extras || []) {
      n += 1;
      out.push(heading(2, `${chapter}.${n} ${x.title}`, `sec_${chapter}_${n}`));
      out.push(...blocks(x.blocks));
    }
  }
  if (index.length !== C.TOOL_COUNT) throw new Error(`guide describes ${index.length} tools, TOOL_COUNT is ${C.TOOL_COUNT}`);
  const ids = new Set(index.map((x) => x.t.id));
  if (ids.size !== index.length) throw new Error('a tool is described twice');

  // Appendix A: tool index
  out.push(heading(1, 'Appendix A. Tool index', 'app_a', { newPage: true }));
  out.push(
    para(
      `All ${C.TOOL_COUNT} tools in version ${C.VERSION}, in the order of this guide. “Internet” shows whether a tool needs it: No (works offline), Optional (uses live data when available, otherwise a bundled or cached copy), or Yes.`,
    ),
  );
  const yn = { no: 'No', optional: 'Optional', yes: 'Yes' };
  out.push(
    ...table({
      caption: `Table 4. All ${C.TOOL_COUNT} tools`,
      widths: [900, 5660, 1300, 1500],
      header: ['Section', 'Task and tool name', 'Group', 'Internet'],
      rows: index.map(({ num, t }) => [
        num,
        [
          new Paragraph({ style: 'TableText', children: runs(t.title) }),
          new Paragraph({ style: 'TableText', children: [new TextRun({ text: t.id, font: MONO, size: 17, color: ACCENT })] }),
        ],
        t.toolset,
        yn[t.internet],
      ]),
    }),
  );

  // Appendix B: glossary
  out.push(heading(1, 'Appendix B. Glossary', 'app_b', { newPage: true }));
  out.push(...table({ caption: 'Table 5. Terms used in this guide', widths: [2600, 6760], header: ['Term', 'Meaning'], rows: C.glossary }));

  // Appendix C: troubleshooting
  out.push(heading(1, 'Appendix C. Troubleshooting and help', 'app_c', { newPage: true }));
  out.push(...table({ caption: 'Table 6. Common problems', widths: [2800, 6560], header: ['Problem', 'What to do'], rows: C.troubleshooting }));
  out.push(
    para(
      'To report a problem, suggest a correction to the reference content, or ask for a new tool, open an issue on the project’s GitHub page (github.com/SpiroGanas/Public-Health-Nursing-MCP-Server). Do not include any identifiable information in an issue.',
    ),
  );
  return out;
}

// ---------------------------------------------------------------------------------------
// Table of contents
// ---------------------------------------------------------------------------------------

function tocParagraphs(headings, pageMap) {
  const out = [new Paragraph({ style: 'TOCHeading', pageBreakBefore: true, children: [new TextRun('Contents')] })];
  for (const h of headings) {
    const page = pageMap && pageMap[h.bookmark] ? String(pageMap[h.bookmark]) : '';
    out.push(
      new Paragraph({
        style: h.level === 1 ? 'TOC1' : 'TOC2',
        children: [
          new InternalHyperlink({
            anchor: h.bookmark,
            children: [
              new TextRun(h.text),
              new TextRun({ children: [new Tab(), page] }),
            ],
          }),
        ],
      }),
    );
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// Styles, numbering, footer
// ---------------------------------------------------------------------------------------

const styles = {
  default: {
    document: { run: { font: FONT, size: 22, color: TEXT, language: { value: 'en-US' } }, paragraph: { spacing: { after: 120, line: 276, lineRule: LineRuleType.AUTO } } },
    title: { run: { font: FONT, size: 56, bold: true, color: NAVY }, paragraph: { spacing: { before: 2400, after: 240, line: 240, lineRule: LineRuleType.AUTO } } },
    heading1: {
      run: { font: FONT, size: 36, bold: true, color: NAVY },
      paragraph: {
        spacing: { before: 0, after: 240 },
        keepNext: true,
        keepLines: true,
        border: { bottom: { style: BorderStyle.SINGLE, size: 12, color: RULE, space: 6 } },
      },
    },
    heading2: { run: { font: FONT, size: 28, bold: true, color: NAVY }, paragraph: { spacing: { before: 360, after: 100 }, keepNext: true, keepLines: true } },
    heading3: { run: { font: FONT, size: 23, bold: true, color: ACCENT }, paragraph: { spacing: { before: 200, after: 60 }, keepNext: true, keepLines: true } },
  },
  paragraphStyles: [
    { id: 'Subtitle', name: 'Subtitle', basedOn: 'Normal', next: 'Normal', run: { font: FONT, size: 36, color: ACCENT }, paragraph: { spacing: { after: 480 } } },
    { id: 'CoverMeta', name: 'Cover Meta', basedOn: 'Normal', run: { size: 22, color: MUTED }, paragraph: { spacing: { after: 360 } } },
    { id: 'CoverText', name: 'Cover Text', basedOn: 'Normal', run: { size: 24 }, paragraph: { spacing: { after: 480, line: 300, lineRule: LineRuleType.AUTO } } },
    {
      id: 'Note',
      name: 'Note',
      basedOn: 'Normal',
      paragraph: {
        indent: { left: 240, right: 240 },
        spacing: { before: 120, after: 120 },
        shading: { type: ShadingType.CLEAR, color: 'auto', fill: LIGHT_FILL },
        border: { left: { style: BorderStyle.SINGLE, size: 24, color: RULE, space: 8 } },
        keepLines: true,
      },
    },
    {
      id: 'Example',
      name: 'Example Request',
      basedOn: 'Normal',
      paragraph: {
        indent: { left: 360, right: 360 },
        spacing: { before: 0, after: 80 },
        shading: { type: ShadingType.CLEAR, color: 'auto', fill: FACT_FILL },
        border: { left: { style: BorderStyle.SINGLE, size: 18, color: RULE, space: 8 } },
        keepLines: true,
      },
    },
    {
      id: 'ToolFacts',
      name: 'Tool Facts',
      basedOn: 'Normal',
      run: { size: 19, color: TEXT },
      paragraph: {
        spacing: { before: 60, after: 160, line: 264, lineRule: LineRuleType.AUTO },
        shading: { type: ShadingType.CLEAR, color: 'auto', fill: FACT_FILL },
        border: {
          top: { style: BorderStyle.SINGLE, size: 4, color: GRID, space: 4 },
          bottom: { style: BorderStyle.SINGLE, size: 4, color: GRID, space: 4 },
        },
        keepNext: true,
        keepLines: true,
      },
    },
    {
      id: 'FooterText',
      name: 'Footer Text',
      basedOn: 'Normal',
      run: { size: 16, color: MUTED },
      paragraph: { spacing: { after: 0 }, tabStops: [{ type: TabStopType.RIGHT, position: CONTENT_W }] },
    },
    { id: 'ListText', name: 'List Text', basedOn: 'Normal', paragraph: { spacing: { after: 60 } } },
    { id: 'TableText', name: 'Table Text', basedOn: 'Normal', run: { size: 19 }, paragraph: { spacing: { after: 40, line: 252, lineRule: LineRuleType.AUTO } } },
    { id: 'Caption', name: 'caption', basedOn: 'Normal', next: 'Normal', run: { size: 20, bold: true, color: NAVY }, paragraph: { spacing: { before: 120, after: 80 }, keepNext: true } },
    { id: 'AfterTable', name: 'After Table', basedOn: 'Normal', run: { size: 8 }, paragraph: { spacing: { before: 0, after: 120, line: 120, lineRule: LineRuleType.AUTO } } },
    { id: 'TOCHeading', name: 'TOC Heading', basedOn: 'Normal', next: 'Normal', run: { size: 36, color: NAVY }, paragraph: { spacing: { after: 240 } } },
    {
      id: 'TOC1',
      name: 'toc 1',
      basedOn: 'Normal',
      next: 'Normal',
      run: { size: 21, color: NAVY },
      paragraph: { spacing: { before: 120, after: 20 }, tabStops: [{ type: TabStopType.RIGHT, position: CONTENT_W, leader: LeaderType.DOT }] },
    },
    {
      id: 'TOC2',
      name: 'toc 2',
      basedOn: 'Normal',
      next: 'Normal',
      run: { size: 20 },
      paragraph: { indent: { left: 360 }, spacing: { after: 0 }, tabStops: [{ type: TabStopType.RIGHT, position: CONTENT_W, leader: LeaderType.DOT }] },
    },
  ],
};

const numbering = {
  config: [
    {
      reference: 'bullets',
      levels: [{ level: 0, format: LevelFormat.BULLET, text: '•', alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: 400, hanging: 260 } } } }],
    },
    {
      reference: 'steps',
      levels: [{ level: 0, format: LevelFormat.DECIMAL, text: '%1.', alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: 440, hanging: 340 } } } }],
    },
  ],
};

function footer() {
  return new Footer({
    children: [
      new Paragraph({
        style: 'FooterText',
        children: [
          new TextRun(`Public Health Nursing MCP Server User Guide, version ${C.VERSION}`),
          new TextRun({ children: [new Tab(), 'Page ', PageNumber.CURRENT, ' of ', PageNumber.TOTAL_PAGES] }),
        ],
      }),
    ],
  });
}

// ---------------------------------------------------------------------------------------
// Assemble, post-process, paginate
// ---------------------------------------------------------------------------------------

async function render(pageMap) {
  stepsInstance = 0;
  const { headings, heading } = makeHeadingCollector();
  const body = buildBody(heading);
  const doc = new Document({
    title: 'Public Health Nursing MCP Server User Guide',
    subject: 'User guide for public health nurses',
    creator: 'Public Health Nursing MCP Server project',
    description: `User guide for version ${C.VERSION}: what each tool does, when to use it, and how to ask for it.`,
    keywords: 'public health nursing, MCP, Massachusetts, user guide',
    styles,
    numbering,
    sections: [
      {
        properties: {
          titlePage: true,
          page: { size: { width: PAGE_W, height: 15840 }, margin: { top: MARGIN, right: MARGIN, bottom: MARGIN, left: MARGIN, footer: 720 } },
        },
        footers: { default: footer(), first: new Footer({ children: [new Paragraph({ children: [] })] }) },
        children: [...titlePage(), ...tocParagraphs(headings, pageMap), ...body],
      },
    ],
  });
  const buf = await Packer.toBuffer(doc);
  return { buf: await postProcess(buf), headings };
}

// Wrap the contents entries in a Word TOC field (so Word can refresh them), make the
// document language explicit in the settings, and drop folder entries from the ZIP.
async function postProcess(buf) {
  const zip = await JSZip.loadAsync(buf);
  let xml = await zip.file('word/document.xml').async('string');
  const paras = [...xml.matchAll(/<w:p>(?:(?!<w:p>).)*?<w:pStyle w:val="TOC[12]"\/>(?:(?!<w:p>).)*?<\/w:p>/gs)];
  if (paras.length === 0) throw new Error('no table of contents entries found');
  const first = paras[0];
  const last = paras[paras.length - 1];
  const begin =
    '<w:r><w:fldChar w:fldCharType="begin"/></w:r>' +
    '<w:r><w:instrText xml:space="preserve"> TOC \\o "1-2" \\h \\z \\u </w:instrText></w:r>' +
    '<w:r><w:fldChar w:fldCharType="separate"/></w:r>';
  const end = '<w:r><w:fldChar w:fldCharType="end"/></w:r>';
  // Insert the end first so the first match's offsets stay valid.
  const lastEnd = last.index + last[0].length - '</w:p>'.length;
  xml = xml.slice(0, lastEnd) + end + xml.slice(lastEnd);
  const firstPPrEnd = xml.indexOf('</w:pPr>', first.index) + '</w:pPr>'.length;
  xml = xml.slice(0, firstPPrEnd) + begin + xml.slice(firstPPrEnd);
  // docx-js gives every bookmark the same id; Word needs each start/end pair to be unique.
  // Bookmarks here never overlap, so each end belongs to the start just before it.
  let nextId = 0;
  let open = null;
  xml = xml.replace(/<w:bookmark(Start|End)\b([^>]*?)w:id="\d+"/g, (_, kind, attrs) => {
    if (kind === 'Start') {
      nextId += 1;
      open = nextId;
      return `<w:bookmarkStart${attrs}w:id="${nextId}"`;
    }
    if (open === null) throw new Error('bookmark end without a start');
    const id = open;
    open = null;
    return `<w:bookmarkEnd${attrs}w:id="${id}"`;
  });
  zip.file('word/document.xml', xml, { createFolders: false });
  // zip.remove() on a folder would delete its contents too, so drop only the folder entries.
  for (const name of Object.keys(zip.files)) if (zip.files[name].dir) delete zip.files[name];
  return zip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE' });
}

function haveTool(cmd) {
  try {
    execFileSync('sh', ['-c', `command -v ${cmd}`], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

// Convert to PDF with LibreOffice and find the page on which each heading starts.
function paginate(docxPath, headings) {
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'phn-guide-'));
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'phn-lo-'));
  try {
    execFileSync(
      'soffice',
      [`-env:UserInstallation=file://${profile}`, '--headless', '--convert-to', 'pdf', '--outdir', work, docxPath],
      { stdio: 'ignore', env: { ...process.env, SAL_USE_VCLPLUGIN: 'svp' } },
    );
    const pdf = path.join(work, path.basename(docxPath).replace(/\.docx$/, '.pdf'));
    const text = execFileSync('pdftotext', [pdf, '-'], { maxBuffer: 64 * 1024 * 1024 }).toString('utf8');
    const norm = (s) => s.replace(/\s+/g, ' ').trim();
    const pages = text.split('\f').map(norm);
    const startPage = pages.findIndex((p, i) => i > 0 && p.startsWith(norm(headings[0].text)));
    if (startPage < 0) throw new Error('could not find the first chapter in the PDF');
    const map = {};
    let cursor = startPage;
    for (const h of headings) {
      const needle = norm(h.text);
      let found = -1;
      for (let i = cursor; i < pages.length; i += 1) {
        if (pages[i].includes(needle)) {
          found = i;
          break;
        }
      }
      if (found < 0) throw new Error(`heading not found in PDF: ${h.text}`);
      map[h.bookmark] = found + 1;
      cursor = found;
    }
    return { map, pageCount: pages.filter((p) => p.length > 0).length };
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
    fs.rmSync(profile, { recursive: true, force: true });
  }
}

async function main() {
  const out = path.resolve(process.argv[2] || path.join(__dirname, 'PHN-MCP-Server-User-Guide.docx'));
  let { buf, headings } = await render(null);
  fs.writeFileSync(out, buf);
  if (!haveTool('soffice') || !haveTool('pdftotext')) {
    console.error('LibreOffice or pdftotext not found: table of contents page numbers left blank.');
    console.error(`Wrote ${out}`);
    return;
  }
  let map = paginate(out, headings).map;
  for (let pass = 0; pass < 3; pass += 1) {
    ({ buf, headings } = await render(map));
    fs.writeFileSync(out, buf);
    const check = paginate(out, headings);
    const same = headings.every((h) => check.map[h.bookmark] === map[h.bookmark]);
    map = check.map;
    if (same) {
      console.error(`Wrote ${out} (${check.pageCount} pages, ${headings.length} contents entries)`);
      return;
    }
  }
  throw new Error('table of contents page numbers did not settle after 3 passes');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
