# User guide

[`PHN-MCP-Server-User-Guide.docx`](PHN-MCP-Server-User-Guide.docx) is the user guide for
public health nurses: an executive summary of what the server offers, how it works and protects
privacy, how to get started, and, for every implemented tool, what it does, when to use it, how
to ask for it, what it returns, and what to keep in mind. It covers only tools that are
implemented and registered in the server (97 in version 0.13.0).

## Files

| File | What it is |
|---|---|
| `PHN-MCP-Server-User-Guide.docx` | The guide (Word, US Letter, about 116 pages). Built; do not edit by hand. |
| `guide-content.js` | All of the guide's text: chapters, one entry per tool, glossary, troubleshooting. |
| `build-user-guide.js` | Turns the content into Word: styles, tables, table of contents, footer. |

## Rebuilding

Requirements: Node.js, the `docx` npm package (`npm install -g docx` if it is not already
installed), and, for the table of contents page numbers, LibreOffice (`soffice`) and Poppler
(`pdftotext`). From the repository root:

```sh
node docs/user-guide/build-user-guide.js
```

The script lays the document out twice: it converts the first pass to PDF to find the page of
each heading, then writes those page numbers into the table of contents. The contents are a real
Word TOC field, so Word can refresh them too (right-click the contents, then Update Field).
Without LibreOffice the guide is still written, with the page numbers left blank.

After rebuilding, check the result with the server's own accessibility checker
(`check_document_accessibility`); the current guide passes every automated check.

## When a tool is added or changed

1. Add or edit the tool's entry in the right chapter of `guide-content.js`, using the same
   fields as the others, and update `TOOL_COUNT` and any counts in the front matter (the
   executive summary's work-area table).
2. Rebuild the guide and commit both the content and the `.docx`.

The test `user_guide_lists_exactly_the_implemented_tools` in
`crates/phn-tools/src/registry/mod.rs` fails when the guide is missing a registered tool,
describes a tool that is not registered, gives a tool the wrong toolset, or has the wrong
`TOOL_COUNT`.

## Accuracy

The guide describes what each tool does from `docs/tools.md` and the code. It does not state
clinical or regulatory rules itself; those come from the tools' reference content, which is
pending subject-matter review. If the reference content changes in a way that affects what a
tool returns, update the guide's description.
