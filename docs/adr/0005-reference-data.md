# ADR 0005: Massachusetts reference data and reportable-condition content

- Status: accepted (Phase 0)

## Context

Phase 0 needs the 351 towns, counties, HMCC regions, and school districts, and the 105 CMR
300.000 reportable disease list. During development, mass.gov and census.gov were not reachable
from the build environment; the MassGIS download bucket was.

## Decision

- **Towns and counties** come from the MassGIS municipal boundary layer (TOWNSSURVEY_POLY).
- **HMCC regions** are derived by locating each town's interior point in the MassGIS DPH
  Emergency Preparedness Region layer (2005) and merging Regions 4A and 4B into 4AB. An
  independent area-weighted spatial join gave identical results for all 351 towns. Every answer
  that uses the HMCC region says it is derived and should be confirmed with the HMCC; the nurse
  can override it.
- **School districts** come from the MassGIS district layer (municipal and regional academic)
  plus charter and vocational-technical districts, keyed by DESE organization code.
- `cargo xtask build-snapshots` reproduces all of this from the source downloads and records
  checksums; a weekly CI job reports upstream changes.
- **Reportable conditions** were drafted from the developer's knowledge of 105 CMR 300.100,
  300.140, and 300.170 because the regulation text could not be retrieved. The content is marked
  `status: pending`, each entry carries a confidence level, every answer states that the list is
  a draft, and the review is due before the first pilot.
- **Update, 2026-11-07:** mass.gov became reachable. The reportable conditions were re-checked
  against the regulation text (Mass. Register #1470, 5/27/22; 300.180, not 300.140, holds the
  diseases reportable directly to DPH) and DPH's current reporting lists, and each entry now
  cites its sections. The content stays `pending` until the subject matter expert reviews it.
  The HMCC column was compared town by town with DPH's HMCC Regional Boundaries map (December
  2019); the one difference (Wilmington, Region 3) is an override in the snapshot builder.

## Consequences

The nurse's review of `content/cd/reportable_conditions.yaml` against the current regulation is
required before Pilot 1. Until then, `lookup_reportable_condition` is safe to demonstrate but not
to rely on.
