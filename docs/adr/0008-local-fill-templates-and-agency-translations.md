# ADR 0008: Templates filled locally, with agency translations

- Status: accepted (decided by the product owner for Phases 7–12, 2026-11-08)
- Phase: 7 (school health: referral letters, health plans, concussion letters); later phases
  that produce documents about a person can reuse the same module

## Context

Phase 7 tools produce documents about individual students: screening referral letters,
individualized health plans and emergency action plans, graduated reentry plans, and caregiver
letters. Everything a tool returns is sent to the AI vendor, so student details must never pass
through tool arguments or results. Families need letters in their own language, but
machine-drafted translations of health letters are not acceptable without review.

## Decision

1. **Template + local fill.** Tools return the document structure and its `{{placeholders}}` to
   the AI. When the nurse points a tool at a local CSV/XLSX in `/data` with the person's details,
   the tool fills the template and writes the documents to `/data/output` only. The result lists
   only file names (generic, numbered) and counts (documents, blank fields, languages). Tools do
   not accept identifying details as arguments.
2. **English only in the server.** Bundled templates are English Markdown with placeholders.
3. **Agency translations.** An agency adds approved translations as
   `/data/templates/<language>/<kind>.md`. `<language>` matches the language value in the nurse's
   file by name or code (`spanish` or `es`, `portuguese` or `pt`, `haitian-creole` or `ht`, and
   others listed in `crates/phn-tools/src/school/templates.rs`). Each run writes the English
   template next to the documents as `template_english_<kind>.md` for translators.
4. **Fallback is visible.** When a family's language has no agency translation, the English
   template is used and the result counts the fallbacks by language.
5. **No machine translation**, ever.

The shared code is `crates/phn-tools/src/school/templates.rs` (`TemplateResolver`, `fill`,
`document`, `combine`, `output_folder`, `write_in`, `translation_help`).

## Consequences

- Agencies control the wording and translation of everything sent to families; the server stays
  free of unreviewed translations.
- A translated template must keep the placeholders; unknown placeholders are left visible as
  `[name]` and reported so the nurse can fix the template.
- PDF output cannot show scripts outside the bundled font; DOCX is the default for letters.
