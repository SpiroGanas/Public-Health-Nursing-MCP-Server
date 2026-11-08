# ADR 0007: Default basis for vaccine recommendations

- Status: accepted (decided by the product owner, 2026-11-07)
- Phase: 5 (recommendations, schedules), 6 (forecasting)

## Context

Since 2025, recommendations from CDC/ACIP have diverged from those of the American Academy of
Pediatrics (AAP), the American College of Obstetricians and Gynecologists (ACOG), the American
Academy of Family Physicians (AAFP), and Massachusetts DPH for several vaccines. The plan already
said the default follows DPH; it did not say what to do when DPH has no stated position.

## Decision

1. Default: DPH's position where DPH has one; otherwise the professional societies (AAP for
   children and adolescents, ACOG for pregnancy, AAFP for adults); then CDC/ACIP.
2. Every answer shows each body's position with its date side by side and highlights differences,
   whatever the default.
3. The basis is a setting (`vaccine_recommendation_basis` via `configure_jurisdiction`):
   `dph_then_professional_then_cdc` (default), `cdc_acip`, `professional_societies`, or `show_all`.
4. The CDSi forecaster (`forecast_immunizations_due`) encodes CDC/ACIP logic only, as published
   by CDC; its answers say so and point to `compare_vaccine_recommendations` where bodies differ.

## Consequences

- Recommendation content must be refreshed whenever any of these bodies publishes a change, and
  each entry carries its source date and review status.
- Forecasts and recommendation comparisons can legitimately disagree; the caveats make that
  explicit rather than hiding it.
