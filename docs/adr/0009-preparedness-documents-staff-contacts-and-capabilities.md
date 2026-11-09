# ADR 0009: Preparedness documents: staff contacts, forms, capabilities, and languages

- Status: accepted (decided by the product owner for Phase 13, 2026-11-09)
- Phase: 13 (emergency preparedness II: ICS forms, COOP, HSEEP exercises, after-action
  reports, CERC)

## Context

Phase 13 tools write the documents a health department produces before, during, and after an
incident. Unlike earlier phases, the people named in these documents are mostly **staff and
partners** (incident commanders, section chiefs, successors, exercise players), not clients.
Their names are routine in ICS forms, but their cell phone numbers and personal emails are not
something to send to the AI vendor. Exercise objectives and after-action findings need a
capability framework, and crisis messages need to reach residents in their own languages
without bundled machine translations (ADR 0008).

## Decision

1. **Staff contacts stay local.** Staff names and positions may be passed in tool arguments.
   Phone numbers, emails, radio call signs, and other contact details come only from a roster
   CSV/XLSX in `/data` (`roster_path`). Tools fill them into documents written to
   `/data/output` and return only counts (`epr::roster::RosterSummary`); nothing read from the
   roster, names included, is returned. When contact details appear in arguments, the tool
   reports their categories in the privacy block and asks the nurse to use the roster instead.
   Shared code: `crates/phn-tools/src/epr/roster.rs`.
2. **ICS forms.** `generate_ics_forms` produces the nine forms in the plan (201, 202, 203, 204,
   205A, 206, 213, 214, 215A) plus 205, 207, 208, and 213RR, so a standard Incident Action Plan
   (202, 203, 204, 205, 205A, 206, 207, 208) can be assembled and resource requests are covered.
   The forms are facsimiles that keep FEMA's titles, block numbers, and block labels (NIMS ICS
   Forms Booklet, v3/v3.1); they are not FEMA's fillable PDFs.
3. **Capabilities.** Exercise objectives and after-action observations map to the 15 CDC Public
   Health Emergency Preparedness and Response (PHEP) capabilities and to the matching FEMA
   Core Capabilities, so one after-action report serves CDC reporting and the HSEEP template.
   ASPR's health care preparedness (HPP) capabilities are not included.
4. **Exercise scenarios.** The tabletop scenarios for Pilot 3 (measles outbreak, coastal storm
   with power outage, and extreme heat) are fully developed (modules, injects, discussion
   questions); other hazards are offered as scenario seeds.
5. **After-action notes.** The nurse shares de-identified hotwash notes in chat; the AI passes
   structured strengths, areas for improvement, and corrective actions; the tool scans that text
   for client identifiers and warns. Participant feedback-form ratings in a CSV/XLSX in `/data`
   are totaled locally (aggregates only).
6. **Crisis messages in other languages.** `crisis_risk_communication_builder` writes English
   messages, lists the town's priority languages (EQ-03), uses agency-approved translations in
   `/data/templates/<language>/` when present, and otherwise writes a translator brief per
   language (as in COMM-02). No machine translation (ADR 0008).

## Consequences

- An agency keeps one staff roster file and reuses it across ICS, COOP, and exercise documents.
- Documents in `/data/output` contain staff contact details and must be handled like other
  internal emergency-plan documents.
- Form facsimiles may differ in layout from FEMA's PDFs; block numbers and labels match so
  staff trained on the official forms can use them. Agencies required to submit FEMA's own PDFs
  copy the content across.
- Capability content (PHEP 2018 standards, FEMA Core Capabilities) and the HSEEP and CERC
  material are `status: pending` until the subject matter expert reviews them.
