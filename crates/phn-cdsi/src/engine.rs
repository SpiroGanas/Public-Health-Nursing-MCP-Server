//! The CDSi processing model (Logic Specification v4.6, Chapters 4 to 9).
//!
//! 1. Organize the immunization history into antigen doses (§4.2).
//! 2. Select relevant patient series (Chapter 5).
//! 3. Evaluate each dose against target doses (Chapter 6) and forecast the
//!    next target dose (Chapter 7) for every relevant patient series.
//! 4. Select the best patient series per antigen (Chapter 8).
//! 5. Combine them into vaccine group forecasts (Chapter 9).
//!
//! Business rule identifiers from the specification are cited in comments.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::data::{
    AgeSpec, AntigenData, ConditionType, CountLogic, Logic, Series, SeriesDose, SeriesType,
    SkipCondition, SkipContext, SupportingData, normalize_cvx,
};
use crate::duration::{Span, max_date, min_date};
use crate::model::*;

/// Run the full evaluation and forecast for one patient.
pub fn run(patient: &PatientInput) -> EngineResult {
    run_with(SupportingData::get(), patient)
}

/// Run with explicit supporting data (for tests).
pub fn run_with(sd: &SupportingData, patient: &PatientInput) -> EngineResult {
    let ctx = Ctx::new(sd, patient);
    let mut result = EngineResult {
        assessment_date: patient.assessment_date,
        patient_series: Vec::new(),
        vaccine_groups: Vec::new(),
        unrecognized_cvx: Vec::new(),
        contraindications: Vec::new(),
        immunity: Vec::new(),
    };
    let organized = ctx.organize(&mut result.unrecognized_cvx);
    // best series per antigen, as indices into result.patient_series
    let mut best_by_antigen: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for antigen in sd.antigens.values() {
        let doses = organized.get(&antigen.name).cloned().unwrap_or_default();
        let mut relevant: Vec<usize> = antigen
            .series
            .iter()
            .enumerate()
            .filter(|(_, s)| ctx.is_relevant(s))
            .map(|(i, _)| i)
            .collect();
        if relevant.is_empty() {
            continue;
        }
        // Series groups referenced by "Completed Series" conditions are
        // processed first (they always have lower group numbers).
        relevant.sort_by_key(|&i| group_key(&antigen.series[i].series_group));
        let immunity = ctx.immunity(antigen);
        if let Some(reason) = &immunity {
            result.immunity.push(ClinicalNote {
                antigen: antigen.name.clone(),
                observation_code: String::new(),
                text: reason.clone(),
            });
        }
        for ci in &antigen.antigen_contraindications {
            if ctx.antigen_ci_applies(ci) {
                result.contraindications.push(ClinicalNote {
                    antigen: antigen.name.clone(),
                    observation_code: ci.observation.code.clone(),
                    text: ci.text.clone(),
                });
            }
        }
        for ci in &antigen.vaccine_contraindications {
            if ctx.observed(&ci.observation.code) {
                result.contraindications.push(ClinicalNote {
                    antigen: antigen.name.clone(),
                    observation_code: ci.observation.code.clone(),
                    text: ci.text.clone(),
                });
            }
        }
        let mut done: Vec<PatientSeries> = Vec::new();
        for &si in &relevant {
            // Series group -> date its earliest complete series was completed
            // (the date of the dose that satisfied its last satisfied target).
            let mut completed = Completed::new();
            for p in done.iter().filter(|p| p.status == SeriesStatus::Complete) {
                let when = p
                    .targets
                    .iter()
                    .filter_map(|t| t.satisfied_by)
                    .map(|j| doses[j].date)
                    .max()
                    .unwrap_or(ctx.assess);
                let e = completed
                    .entry(p.series.series_group.clone())
                    .or_insert(when);
                *e = (*e).min(when);
            }
            let mut ps = PatientSeries::new(antigen, si, doses.len());
            ctx.evaluate(&mut ps, &doses, &completed);
            ctx.forecast(&mut ps, &doses, &completed, immunity.is_some());
            done.push(ps);
        }
        let best = ctx.select_best(&done, &doses);
        let base = result.patient_series.len();
        for (k, ps) in done.iter().enumerate() {
            result
                .patient_series
                .push(ctx.series_result(ps, &doses, best.contains(&k)));
        }
        best_by_antigen.insert(
            antigen.name.clone(),
            best.iter().map(|&k| base + k).collect(),
        );
    }
    result.vaccine_groups = ctx.vaccine_groups(&result.patient_series, &best_by_antigen);
    result
}

/// Series group -> date a series in it was completed.
type Completed = BTreeMap<String, NaiveDate>;

fn group_key(g: &str) -> (u32, String) {
    (g.parse().unwrap_or(u32::MAX), g.to_string())
}

// ----------------------------------------------------------------- context

struct Ctx<'a> {
    sd: &'a SupportingData,
    p: &'a PatientInput,
    dob: NaiveDate,
    assess: NaiveDate,
    /// Patient doses sorted by date (stable), as indices into `p.doses`.
    sorted: Vec<usize>,
    obs: BTreeMap<String, Option<NaiveDate>>,
}

/// One antigen administered record.
#[derive(Debug, Clone)]
struct AntigenDose {
    /// Index into `PatientInput::doses`.
    input: usize,
    /// Position in the date-sorted patient history.
    pos: usize,
    date: NaiveDate,
    cvx: String,
}

#[derive(Debug, Clone)]
struct Target {
    sd_idx: usize,
    status: TargetStatus,
    satisfied_by: Option<usize>,
}

#[derive(Debug, Clone)]
struct EvalRec {
    status: EvalStatus,
    target: Option<usize>,
    reasons: Vec<String>,
    inadvertent: bool,
}

/// Working state of one relevant patient series.
#[derive(Debug, Clone)]
struct PatientSeries<'a> {
    antigen: &'a AntigenData,
    series: &'a Series,
    targets: Vec<Target>,
    /// Per antigen dose.
    evals: Vec<Option<EvalRec>>,
    /// Index of the target dose being forecast (== targets.len() when none).
    current: usize,
    status: SeriesStatus,
    reasons: Vec<String>,
    dates: ForecastDates,
    recommended: Vec<RecommendedVaccine>,
    /// FORECASTDN-1 forecast dose number, when a dose is forecast.
    forecast_dose_number: Option<usize>,
}

impl<'a> PatientSeries<'a> {
    fn new(antigen: &'a AntigenData, series_idx: usize, n_doses: usize) -> Self {
        let series = &antigen.series[series_idx];
        Self {
            antigen,
            series,
            targets: (0..series.doses.len())
                .map(|i| Target {
                    sd_idx: i,
                    status: TargetStatus::NotSatisfied,
                    satisfied_by: None,
                })
                .collect(),
            evals: vec![None; n_doses],
            current: 0,
            status: SeriesStatus::NotComplete,
            reasons: Vec::new(),
            dates: ForecastDates::default(),
            recommended: Vec::new(),
            forecast_dose_number: None,
        }
    }

    fn dose(&self, t: usize) -> &'a SeriesDose {
        &self.series.doses[self.targets[t].sd_idx]
    }

    fn valid_doses(&self) -> usize {
        self.targets
            .iter()
            .filter(|t| t.status == TargetStatus::Satisfied)
            .count()
    }

    fn status_of(&self, i: usize) -> Option<EvalStatus> {
        self.evals.get(i).and_then(|e| e.as_ref()).map(|e| e.status)
    }
}

/// Phase in which a conditional skip is checked.
#[derive(Clone, Copy)]
enum Phase {
    /// Evaluating the antigen dose at this index.
    Eval(usize),
    /// Forecasting (and validating the forecast).
    Forecast,
}

fn add(span: Option<Span>, date: NaiveDate) -> Option<NaiveDate> {
    span.map(|s| s.add_to(date))
}

fn minus_one_day(d: NaiveDate) -> NaiveDate {
    d.pred_opt().unwrap_or(d)
}

impl<'a> Ctx<'a> {
    fn new(sd: &'a SupportingData, p: &'a PatientInput) -> Self {
        let mut sorted: Vec<usize> = (0..p.doses.len()).collect();
        sorted.sort_by_key(|&i| p.doses[i].date);
        let mut obs = BTreeMap::new();
        for o in &p.observations {
            let code = o.code.trim().to_string();
            let entry = obs.entry(code).or_insert(None);
            if let Some(d) = o.date
                && entry.is_none_or(|e: NaiveDate| d > e)
            {
                *entry = Some(d);
            }
        }
        Self {
            sd,
            p,
            dob: p.dob,
            assess: p.assessment_date,
            sorted,
            obs,
        }
    }

    fn observed(&self, code: &str) -> bool {
        self.obs.contains_key(code.trim())
    }

    fn age_date(&self, span: Option<Span>) -> Option<NaiveDate> {
        add(span, self.dob)
    }

    // ------------------------------------------------- 4.2 organize history

    fn organize(&self, unrecognized: &mut Vec<String>) -> BTreeMap<String, Vec<AntigenDose>> {
        let mut out: BTreeMap<String, Vec<AntigenDose>> = BTreeMap::new();
        for (pos, &input) in self.sorted.iter().enumerate() {
            let d = &self.p.doses[input];
            let cvx = normalize_cvx(&d.cvx);
            let Some(entry) = self.sd.schedule.cvx_map.get(&cvx) else {
                if !unrecognized.contains(&d.cvx) {
                    unrecognized.push(d.cvx.clone());
                }
                continue;
            };
            for a in &entry.associations {
                let begin = self.age_date(a.begin_age).unwrap_or_else(min_date);
                let end = self.age_date(a.end_age).unwrap_or_else(max_date);
                if d.date >= begin && d.date < end {
                    out.entry(a.antigen.clone()).or_default().push(AntigenDose {
                        input,
                        pos,
                        date: d.date,
                        cvx: cvx.clone(),
                    });
                }
            }
        }
        out
    }

    // -------------------------------------------- 5 relevant patient series

    fn is_relevant(&self, s: &Series) -> bool {
        // Table 5-5
        let gender_ok = s.required_gender.is_empty()
            || s.required_gender
                .iter()
                .any(|g| g.eq_ignore_ascii_case(self.p.gender.label()));
        if !gender_ok {
            return false;
        }
        match s.series_type {
            SeriesType::Standard | SeriesType::EvaluationOnly => true,
            SeriesType::Risk => s.indications.iter().any(|ind| {
                // Table 5-4
                self.observed(&ind.observation.code) && {
                    let begin = self.age_date(ind.begin_age).unwrap_or_else(min_date);
                    let end = self.age_date(ind.end_age).unwrap_or_else(max_date);
                    begin <= self.assess && self.assess < end
                }
            }),
        }
    }

    // ------------------------------------------------------ 6 evaluation

    fn age_spec<'s>(&self, dose: &'s SeriesDose, on: NaiveDate) -> Option<&'s AgeSpec> {
        dose.ages.iter().find(|a| a.effective.applies(on))
    }

    /// CALCDTINT-1: the most immediate previous antigen dose (before index
    /// `before`, or any when `None`) with a status of Valid or Not Valid that
    /// was not an inadvertent administration.
    fn previous_ref(
        &self,
        ps: &PatientSeries,
        doses: &[AntigenDose],
        before: Option<usize>,
    ) -> Option<usize> {
        let end = before.unwrap_or(doses.len()).min(doses.len());
        (0..end).rev().find(|&j| {
            ps.evals[j].as_ref().is_some_and(|e| {
                matches!(e.status, EvalStatus::Valid | EvalStatus::NotValid) && !e.inadvertent
            })
        })
    }

    /// Reference date of an interval (CALCDTINT-1, -2, -8, -9).
    #[allow(clippy::too_many_arguments)]
    fn interval_ref(
        &self,
        ps: &PatientSeries,
        doses: &[AntigenDose],
        phase: Phase,
        from_previous: bool,
        from_target: Option<usize>,
        from_most_recent: &[String],
        from_obs: Option<&String>,
    ) -> Option<NaiveDate> {
        let before = match phase {
            Phase::Eval(i) => Some(i),
            Phase::Forecast => None,
        };
        if from_previous {
            return self.previous_ref(ps, doses, before).map(|j| doses[j].date);
        }
        if let Some(n) = from_target {
            return ps
                .targets
                .get(n.checked_sub(1)?)
                .and_then(|t| t.satisfied_by)
                .map(|j| doses[j].date);
        }
        if !from_most_recent.is_empty() {
            // Most recent dose of one of the listed vaccine types in the
            // whole history, before the dose being evaluated.
            let limit_pos = match phase {
                Phase::Eval(i) => doses[i].pos,
                Phase::Forecast => usize::MAX,
            };
            let mut best: Option<NaiveDate> = None;
            for (pos, &input) in self.sorted.iter().enumerate() {
                if pos >= limit_pos {
                    break;
                }
                let d = &self.p.doses[input];
                if !from_most_recent.contains(&normalize_cvx(&d.cvx)) {
                    continue;
                }
                // Not an inadvertent administration in this series.
                let inadvertent = doses
                    .iter()
                    .position(|ad| ad.pos == pos)
                    .and_then(|j| ps.evals[j].as_ref())
                    .is_some_and(|e| e.inadvertent);
                if !inadvertent {
                    best = Some(d.date);
                }
            }
            return best;
        }
        if let Some(code) = from_obs {
            return self.obs.get(code.trim()).copied().flatten();
        }
        None
    }

    fn evaluate(&self, ps: &mut PatientSeries, doses: &[AntigenDose], completed: &Completed) {
        let mut t = 0usize;
        for i in 0..doses.len() {
            if t >= ps.targets.len() {
                ps.evals[i] = Some(EvalRec {
                    status: EvalStatus::Extraneous,
                    target: None,
                    reasons: vec!["Series already complete".into()],
                    inadvertent: false,
                });
                continue;
            }
            loop {
                let input = &self.p.doses[doses[i].input];
                // 6.1 dose condition (Table 6-3)
                let expired = input.lot_expiration.is_some_and(|e| input.date > e);
                if expired || input.sub_standard {
                    ps.evals[i] = Some(EvalRec {
                        status: EvalStatus::SubStandard,
                        target: Some(t),
                        reasons: vec![if expired {
                            "Administered after the lot expiration date".into()
                        } else {
                            "Dose condition (e.g., expired, sub-potent, recalled)".into()
                        }],
                        inadvertent: false,
                    });
                    break;
                }
                // 6.2 conditional skip
                if self.can_skip(ps, doses, t, Phase::Eval(i), doses[i].date, completed) {
                    ps.targets[t].status = TargetStatus::Skipped;
                    t += 1;
                    if t >= ps.targets.len() {
                        ps.evals[i] = Some(EvalRec {
                            status: EvalStatus::Extraneous,
                            target: None,
                            reasons: vec!["Series already complete".into()],
                            inadvertent: false,
                        });
                        break;
                    }
                    continue;
                }
                let rec = self.evaluate_dose(ps, doses, i, t);
                let valid = rec.status == EvalStatus::Valid;
                ps.evals[i] = Some(rec);
                if valid {
                    ps.targets[t].status = TargetStatus::Satisfied;
                    ps.targets[t].satisfied_by = Some(i);
                    if ps.dose(t).recurring {
                        let copy = Target {
                            sd_idx: ps.targets[t].sd_idx,
                            status: TargetStatus::NotSatisfied,
                            satisfied_by: None,
                        };
                        ps.targets.insert(t + 1, copy);
                    }
                    t += 1;
                }
                break;
            }
        }
        ps.current = t;
    }

    /// Evaluate one dose against target dose `t` (6.3 to 6.10).
    fn evaluate_dose(
        &self,
        ps: &PatientSeries,
        doses: &[AntigenDose],
        i: usize,
        t: usize,
    ) -> EvalRec {
        let sdose = ps.dose(t);
        let ad = &doses[i];
        let input = &self.p.doses[ad.input];
        let date = ad.date;
        let mut reasons = Vec::new();

        // 6.3 inadvertent vaccine (Table 6-13)
        if sdose.inadvertent_vaccines.contains(&ad.cvx) {
            return EvalRec {
                status: EvalStatus::NotValid,
                target: Some(t),
                reasons: vec!["Inadvertent administration".into()],
                inadvertent: true,
            };
        }

        // 6.4 age (Table 6-15)
        #[derive(PartialEq)]
        enum AgeOutcome {
            Valid,
            TooYoung,
            TooOld,
        }
        let age = match self.age_spec(sdose, date) {
            None => AgeOutcome::Valid,
            Some(a) => {
                let abs_min = self.age_date(a.abs_min_age).unwrap_or_else(min_date);
                let min = self.age_date(a.min_age).unwrap_or_else(min_date);
                let max = self.age_date(a.max_age).unwrap_or_else(max_date);
                if date < abs_min {
                    reasons.push("Age: too young".into());
                    AgeOutcome::TooYoung
                } else if date >= max {
                    reasons.push("Age: too old".into());
                    AgeOutcome::TooOld
                } else {
                    if date < min {
                        reasons.push("Age: grace period".into());
                    }
                    AgeOutcome::Valid
                }
            }
        };

        // 6.5 preferable intervals (Table 6-18): all must be satisfied.
        let mut interval_ok = true;
        let mut interval_reasons = Vec::new();
        for iv in sdose
            .intervals
            .iter()
            .filter(|iv| iv.effective.applies(date))
        {
            let Some(r) = self.interval_ref(
                ps,
                doses,
                Phase::Eval(i),
                iv.from_previous,
                iv.from_target_dose,
                &iv.from_most_recent,
                iv.from_relevant_obs.as_ref(),
            ) else {
                continue;
            };
            let abs_min = add(iv.abs_min_int, r).unwrap_or_else(min_date);
            let min = add(iv.min_int, r).unwrap_or_else(min_date);
            if date < abs_min {
                interval_ok = false;
            } else if date < min {
                interval_reasons.push("Interval: grace period".to_string());
            }
        }
        if !interval_ok {
            // 6.6 allowable intervals (Table 6-21); none defined = not valid.
            let allowable: Vec<_> = sdose
                .allowable_intervals
                .iter()
                .filter(|iv| iv.effective.applies(date))
                .collect();
            let allowed = !allowable.is_empty()
                && allowable.iter().all(|iv| {
                    match self.interval_ref(
                        ps,
                        doses,
                        Phase::Eval(i),
                        iv.from_previous,
                        iv.from_target_dose,
                        &[],
                        None,
                    ) {
                        Some(r) => date >= add(iv.abs_min_int, r).unwrap_or_else(min_date),
                        None => true,
                    }
                });
            if allowed {
                interval_ok = true;
                interval_reasons.clear();
                interval_reasons.push("Interval: allowable interval".into());
            } else {
                interval_reasons.clear();
                interval_reasons.push("Interval: too soon".into());
            }
        }
        reasons.extend(interval_reasons);

        // 6.7 vaccine conflict (CONFLICT-3)
        let conflict = self.in_conflict(ps, doses, i);
        if conflict {
            reasons.push("Live virus conflict".into());
        }

        // 6.8 preferable vaccine (Table 6-26), 6.9 allowable vaccine (6-29)
        let mut vaccine_ok = false;
        let mut pref_reason = None;
        for pv in sdose.preferable_vaccines.iter().filter(|p| p.cvx == ad.cvx) {
            let begin = self.age_date(pv.begin_age).unwrap_or_else(min_date);
            let end = self.age_date(pv.end_age).unwrap_or_else(max_date);
            if !(begin <= date && date < end) {
                pref_reason
                    .get_or_insert("Vaccine: preferable vaccine given outside its age range");
                continue;
            }
            let trade_ok = match (&pv.trade_name, &input.trade_name) {
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b.trim()),
                _ => true,
            };
            let mvx_ok = match (&pv.mvx, &input.mvx) {
                (Some(a), Some(b)) if !b.trim().is_empty() => a.eq_ignore_ascii_case(b.trim()),
                _ => true,
            };
            if !(trade_ok && mvx_ok) {
                pref_reason.get_or_insert("Vaccine: trade name is not the preferable product");
                continue;
            }
            vaccine_ok = true;
            if let (Some(req), Some(given)) = (pv.volume, input.volume_ml)
                && given < req
            {
                reasons.push("Volume administered is less than recommended volume".into());
            }
            break;
        }
        if !vaccine_ok {
            for av in sdose.allowable_vaccines.iter().filter(|a| a.cvx == ad.cvx) {
                let begin = self.age_date(av.begin_age).unwrap_or_else(min_date);
                let end = self.age_date(av.end_age).unwrap_or_else(max_date);
                if begin <= date && date < end {
                    vaccine_ok = true;
                    reasons.push("Vaccine: allowable vaccine".into());
                    break;
                }
            }
        }
        if !vaccine_ok {
            reasons.push(
                pref_reason
                    .unwrap_or("Vaccine: not a preferable or allowable vaccine")
                    .to_string(),
            );
        }

        // 6.10 satisfy target dose (Table 6-31)
        let status = if age == AgeOutcome::TooOld {
            EvalStatus::Extraneous
        } else if age == AgeOutcome::TooYoung || !interval_ok || conflict || !vaccine_ok {
            EvalStatus::NotValid
        } else {
            EvalStatus::Valid
        };
        EvalRec {
            status,
            target: Some(t),
            reasons,
            inadvertent: false,
        }
    }

    /// CALCDTCONFLICT-1, CALCDTCONFLICT-2, CONFLICT-3.
    fn in_conflict(&self, ps: &PatientSeries, doses: &[AntigenDose], i: usize) -> bool {
        let cur = &doses[i];
        for c in self
            .sd
            .schedule
            .live_virus_conflicts
            .iter()
            .filter(|c| c.current_cvx == cur.cvx)
        {
            for (pos, &input) in self.sorted.iter().enumerate() {
                if pos == cur.pos {
                    continue;
                }
                let prev = &self.p.doses[input];
                if normalize_cvx(&prev.cvx) != c.previous_cvx {
                    continue;
                }
                let begin = add(c.begin, prev.date).unwrap_or(prev.date);
                // A previous dose of this antigen that was evaluated and not
                // valid gets no grace period; otherwise the minimum end.
                let prev_status = doses
                    .iter()
                    .position(|d| d.pos == pos)
                    .and_then(|j| ps.status_of(j));
                let end_span = match prev_status {
                    Some(s) if s != EvalStatus::Valid => c.end,
                    _ => c.min_end.or(c.end),
                };
                let end = add(end_span, prev.date).unwrap_or(prev.date);
                if begin <= cur.date && cur.date < end {
                    return true;
                }
            }
        }
        false
    }

    // ------------------------------------------------ conditional skip

    fn can_skip(
        &self,
        ps: &PatientSeries,
        doses: &[AntigenDose],
        t: usize,
        phase: Phase,
        reference: NaiveDate,
        completed: &Completed,
    ) -> bool {
        let sdose = ps.dose(t);
        for cs in &sdose.conditional_skips {
            let applies = matches!(
                (phase, cs.context),
                (_, SkipContext::Both)
                    | (Phase::Eval(_), SkipContext::Evaluation)
                    | (Phase::Forecast, SkipContext::Forecast)
            );
            if !applies {
                continue;
            }
            let anchor = match phase {
                Phase::Eval(_) => reference,
                Phase::Forecast => self.assess,
            };
            let results: Vec<bool> = cs
                .sets
                .iter()
                .filter(|s| s.effective.applies(anchor))
                .map(|set| {
                    let met: Vec<bool> = set
                        .conditions
                        .iter()
                        .map(|c| self.condition_met(ps, doses, c, phase, reference, completed))
                        .collect();
                    match set.condition_logic {
                        Logic::And => !met.is_empty() && met.iter().all(|&m| m),
                        Logic::Or => met.iter().any(|&m| m),
                    }
                })
                .collect();
            // Table 6-11
            let skip = match cs.set_logic {
                Logic::And => !results.is_empty() && results.iter().all(|&m| m),
                Logic::Or => results.iter().any(|&m| m),
            };
            if skip {
                return true;
            }
        }
        false
    }

    fn condition_met(
        &self,
        ps: &PatientSeries,
        doses: &[AntigenDose],
        c: &SkipCondition,
        phase: Phase,
        reference: NaiveDate,
        completed: &Completed,
    ) -> bool {
        match c.kind {
            // Table 6-6
            ConditionType::Age => {
                let begin = self.age_date(c.begin_age).unwrap_or_else(min_date);
                let end = self.age_date(c.end_age).unwrap_or_else(max_date);
                end > reference && reference >= begin
            }
            // Table 6-7. A series counts as complete for a dose only if it
            // was completed before that dose was given (on or before the
            // reference date when forecasting).
            ConditionType::CompletedSeries => c.series_groups.iter().any(|g| {
                completed.get(g).is_some_and(|&done| match phase {
                    Phase::Eval(_) => done < reference,
                    Phase::Forecast => done <= reference,
                })
            }),
            // Table 6-8, CALCDTSKIP-5
            ConditionType::Interval => {
                let before = match phase {
                    Phase::Eval(i) => Some(i),
                    Phase::Forecast => None,
                };
                match self.previous_ref(ps, doses, before) {
                    Some(j) => match c.interval {
                        Some(iv) => reference >= iv.add_to(doses[j].date),
                        None => false,
                    },
                    None => false,
                }
            }
            // Table 6-9, CONDSKIP-1
            ConditionType::VaccineCountByAge
            | ConditionType::VaccineCountByDate
            | ConditionType::VaccineCountByDateAndAge => {
                let begin_age = self.age_date(c.begin_age).unwrap_or_else(min_date);
                let end_age = self.age_date(c.end_age).unwrap_or_else(max_date);
                let start = c.start_date.unwrap_or_else(min_date);
                let end = c.end_date.unwrap_or_else(max_date);
                let upto = match phase {
                    Phase::Eval(i) => i,
                    Phase::Forecast => doses.len(),
                };
                let count = (0..upto)
                    .filter(|&j| {
                        let d = &doses[j];
                        if matches!(phase, Phase::Forecast) && d.date > reference {
                            return false;
                        }
                        (c.vaccine_types.is_empty() || c.vaccine_types.contains(&d.cvx))
                            && d.date >= begin_age
                            && d.date < end_age
                            && d.date >= start
                            && d.date < end
                            && (!c.valid_only || ps.status_of(j) == Some(EvalStatus::Valid))
                    })
                    .count() as u32;
                match c.count_logic {
                    CountLogic::GreaterThan => count > c.dose_count,
                    CountLogic::Equal => count == c.dose_count,
                    CountLogic::LessThan => count < c.dose_count,
                }
            }
        }
    }

    // ------------------------------------------------------- 7 forecast

    /// 7.2 evidence of immunity (Table 7-3). Returns the reason when immune.
    fn immunity(&self, antigen: &AntigenData) -> Option<String> {
        for ch in &antigen.immunity.clinical_history {
            if self.observed(&ch.code) {
                return Some(format!("{} ({})", ch.title, ch.code));
            }
        }
        for b in &antigen.immunity.birth_date {
            if self.dob >= b.immunity_birth_date {
                continue;
            }
            if b.exclusions.iter().any(|e| self.observed(&e.code)) {
                continue;
            }
            let country_ok = match &b.birth_country {
                None => true,
                Some(_) => self.p.born_in_us == Some(true),
            };
            if country_ok {
                return Some(format!(
                    "Born before {}{}",
                    b.immunity_birth_date.format("%m/%d/%Y"),
                    b.birth_country
                        .as_ref()
                        .map(|c| format!(" in the {c}"))
                        .unwrap_or_default()
                ));
            }
        }
        None
    }

    fn antigen_ci_applies(&self, ci: &crate::data::AntigenContraindication) -> bool {
        self.observed(&ci.observation.code) && {
            let begin = self.age_date(ci.begin_age).unwrap_or_else(min_date);
            let end = self.age_date(ci.end_age).unwrap_or_else(max_date);
            begin <= self.assess && self.assess < end
        }
    }

    /// Vaccine types (CVX) contraindicated for this patient (Table 7-6).
    fn contraindicated_cvx(&self, antigen: &AntigenData) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for ci in &antigen.vaccine_contraindications {
            if !self.observed(&ci.observation.code) {
                continue;
            }
            for v in &ci.vaccines {
                let begin = self.age_date(v.begin_age).unwrap_or_else(min_date);
                let end = self.age_date(v.end_age).unwrap_or_else(max_date);
                if begin <= self.assess && self.assess < end {
                    out.insert(v.cvx.clone());
                }
            }
        }
        out
    }

    fn forecast(
        &self,
        ps: &mut PatientSeries,
        doses: &[AntigenDose],
        completed: &Completed,
        immune: bool,
    ) {
        let antigen_ci = ps
            .antigen
            .antigen_contraindications
            .iter()
            .any(|ci| self.antigen_ci_applies(ci));
        let ci_cvx = self.contraindicated_cvx(ps.antigen);
        let mut t = ps.current;
        // Guard against pathological skip loops.
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 200 || t >= ps.targets.len() {
                ps.current = ps.targets.len();
                // Table 7-10, rules 2 and 3
                if ps.valid_doses() > 0 {
                    ps.status = SeriesStatus::Complete;
                    ps.reasons.push("Patient series is complete".into());
                } else {
                    ps.status = SeriesStatus::NotRecommended;
                    ps.reasons.push(
                        "Not recommended at this time due to past immunization history".into(),
                    );
                }
                return;
            }
            // 7.1 conditional skip at the assessment date
            if self.can_skip(ps, doses, t, Phase::Forecast, self.assess, completed) {
                ps.targets[t].status = TargetStatus::Skipped;
                t += 1;
                continue;
            }
            ps.current = t;
            let sdose = ps.dose(t);
            // 7.2 immunity, 7.3 contraindication, 7.4 forecast need
            if immune {
                ps.status = SeriesStatus::Immune;
                ps.reasons.push("Patient has evidence of immunity".into());
                return;
            }
            let all_pref_ci = !sdose.preferable_vaccines.is_empty()
                && sdose
                    .preferable_vaccines
                    .iter()
                    .all(|p| ci_cvx.contains(&p.cvx));
            if antigen_ci || all_pref_ci {
                ps.status = SeriesStatus::Contraindicated;
                ps.reasons.push("Patient has a contraindication".into());
                return;
            }
            if let Some(end) = sdose.seasonal.and_then(|s| s.end)
                && self.assess > end
            {
                ps.status = SeriesStatus::NotRecommended;
                ps.reasons
                    .push("Past seasonal recommendation end date".into());
                return;
            }
            let age = self.age_spec(sdose, self.assess);
            let max_age_date = age.and_then(|a| self.age_date(a.max_age));
            if let Some(max) = max_age_date
                && self.assess >= max
            {
                ps.status = SeriesStatus::AgedOut;
                ps.reasons
                    .push("Patient has exceeded the maximum age".into());
                return;
            }
            let earliest = self.candidate_earliest(ps, doses, t);
            if let Some(max) = max_age_date
                && earliest >= max
            {
                ps.status = SeriesStatus::AgedOut;
                ps.reasons
                    .push("Patient is unable to finish the series prior to the maximum age".into());
                return;
            }
            // 7.6 validate recommendation at the earliest date
            if self.can_skip(ps, doses, t, Phase::Forecast, earliest, completed) {
                ps.targets[t].status = TargetStatus::Skipped;
                t += 1;
                continue;
            }
            // 7.5 generate dates
            ps.status = SeriesStatus::NotComplete;
            self.generate_dates(ps, doses, t, earliest, &ci_cvx);
            return;
        }
    }

    /// FORECASTDTCAN-1.
    fn candidate_earliest(&self, ps: &PatientSeries, doses: &[AntigenDose], t: usize) -> NaiveDate {
        let sdose = ps.dose(t);
        let mut dates: Vec<NaiveDate> = Vec::new();
        if let Some(a) = self.age_spec(sdose, self.assess)
            && let Some(d) = self.age_date(a.min_age)
        {
            dates.push(d);
        }
        for iv in sdose
            .intervals
            .iter()
            .filter(|iv| iv.effective.applies(self.assess))
        {
            if let Some(r) = self.interval_ref(
                ps,
                doses,
                Phase::Forecast,
                iv.from_previous,
                iv.from_target_dose,
                &iv.from_most_recent,
                iv.from_relevant_obs.as_ref(),
            ) && let Some(d) = add(iv.min_int, r)
            {
                dates.push(d);
            }
        }
        // CALCDTCONFLICT-3
        let pref: BTreeSet<&String> = sdose.preferable_vaccines.iter().map(|p| &p.cvx).collect();
        for c in self
            .sd
            .schedule
            .live_virus_conflicts
            .iter()
            .filter(|c| pref.contains(&c.current_cvx))
        {
            for d in &self.p.doses {
                if normalize_cvx(&d.cvx) == c.previous_cvx
                    && let Some(end) = add(c.end, d.date)
                {
                    dates.push(end);
                }
            }
        }
        if let Some(start) = sdose.seasonal.and_then(|s| s.start) {
            dates.push(start);
        }
        // Inadvertent doses and the most recent dose evaluated in this series.
        for (j, e) in ps.evals.iter().enumerate() {
            if let Some(e) = e
                && e.target.is_some()
            {
                dates.push(doses[j].date);
            }
        }
        dates.into_iter().max().unwrap_or(self.dob).max(self.dob)
    }

    fn generate_dates(
        &self,
        ps: &mut PatientSeries,
        doses: &[AntigenDose],
        t: usize,
        earliest: NaiveDate,
        ci_cvx: &BTreeSet<String>,
    ) {
        let sdose = ps.dose(t);
        let age = self.age_spec(sdose, self.assess);
        let mut rec_int: Vec<NaiveDate> = Vec::new();
        let mut late_int: Vec<NaiveDate> = Vec::new();
        for iv in sdose
            .intervals
            .iter()
            .filter(|iv| iv.effective.applies(self.assess))
        {
            if let Some(r) = self.interval_ref(
                ps,
                doses,
                Phase::Forecast,
                iv.from_previous,
                iv.from_target_dose,
                &iv.from_most_recent,
                iv.from_relevant_obs.as_ref(),
            ) {
                if let Some(d) = add(iv.earliest_rec_int, r) {
                    rec_int.push(d);
                }
                if let Some(d) = add(iv.latest_rec_int, r) {
                    late_int.push(d);
                }
            }
        }
        // FORECASTDT-2
        let unadj_rec = age
            .and_then(|a| self.age_date(a.earliest_rec_age))
            .or_else(|| rec_int.iter().max().copied())
            .unwrap_or(earliest);
        // FORECASTDT-3
        let unadj_past = age
            .and_then(|a| self.age_date(a.latest_rec_age))
            .or_else(|| late_int.iter().max().copied())
            .map(minus_one_day);
        // FORECASTDT-4
        let latest = age
            .and_then(|a| self.age_date(a.max_age))
            .map(minus_one_day);
        ps.dates = ForecastDates {
            earliest: Some(earliest),
            recommended: Some(unadj_rec.max(earliest)), // FORECASTDT-5
            past_due: unadj_past.map(|p| p.max(earliest)), // FORECASTDT-6
            latest,
            unadjusted_recommended: Some(unadj_rec),
            unadjusted_past_due: unadj_past,
        };
        // FORECASTRECVAC-1
        let rec = ps.dates.recommended.unwrap_or(earliest);
        let mut seen = BTreeSet::new();
        ps.recommended = sdose
            .preferable_vaccines
            .iter()
            .filter(|p| p.forecast_vaccine_type && !ci_cvx.contains(&p.cvx))
            .filter(|p| {
                let begin = self.age_date(p.begin_age).unwrap_or_else(min_date);
                let end = self.age_date(p.end_age).unwrap_or_else(max_date);
                (begin <= earliest && earliest < end) || (begin <= rec && rec < end)
            })
            .filter(|p| seen.insert(p.cvx.clone()))
            .map(|p| RecommendedVaccine {
                cvx: p.cvx.clone(),
                vaccine_type: p.vaccine_type.clone(),
            })
            .collect();
        // FORECASTDN-1
        let n = ps
            .targets
            .iter()
            .filter(|tg| tg.status == TargetStatus::Satisfied)
            .filter(
                |tg| match ps.series.doses[tg.sd_idx].seasonal.and_then(|s| s.start) {
                    None => true,
                    Some(start) => tg.satisfied_by.is_some_and(|j| doses[j].date >= start),
                },
            )
            .count();
        ps.forecast_dose_number = Some(n + 1);
        ps.reasons.push("Patient series is not complete".into());
    }

    // ------------------------------------------- 8 select best patient series

    fn select_best(&self, all: &[PatientSeries], doses: &[AntigenDose]) -> Vec<usize> {
        let mut groups: Vec<String> = all.iter().map(|p| p.series.series_group.clone()).collect();
        groups.sort_by_key(|g| group_key(g));
        groups.dedup();
        // prioritized series per group
        let mut prioritized: Vec<(String, usize)> = Vec::new();
        for g in &groups {
            let members: Vec<usize> = (0..all.len())
                .filter(|&k| &all[k].series.series_group == g)
                .collect();
            if let Some(k) = self.prioritized(all, doses, &members) {
                prioritized.push((g.clone(), k));
            }
        }
        // 8.8 Table 8-14
        let mut best = Vec::new();
        for (g, k) in &prioritized {
            let ps = &all[*k];
            // Equivalent series groups are those the prioritized series
            // itself names (the relation is not assumed to be symmetric).
            let equivalent = |other_g: &String| {
                other_g != g && ps.series.equivalent_series_groups.contains(other_g)
            };
            if ps.status == SeriesStatus::Complete {
                best.push(*k);
                continue;
            }
            let eq_complete = prioritized
                .iter()
                .any(|(og, ok)| equivalent(og) && all[*ok].status == SeriesStatus::Complete);
            if eq_complete || ps.series.series_type == SeriesType::EvaluationOnly {
                continue;
            }
            if ps.series.series_type == SeriesType::Risk {
                best.push(*k);
                continue;
            }
            let eq_risk = prioritized
                .iter()
                .any(|(og, ok)| equivalent(og) && all[*ok].series.series_type == SeriesType::Risk);
            if !eq_risk {
                best.push(*k);
            }
        }
        best
    }

    fn prioritized(
        &self,
        all: &[PatientSeries],
        doses: &[AntigenDose],
        members: &[usize],
    ) -> Option<usize> {
        // SELECTB-24
        let all_ci = members
            .iter()
            .all(|&k| all[k].status == SeriesStatus::Contraindicated);
        let candidate = |k: usize| all[k].status != SeriesStatus::Contraindicated || all_ci;
        let top_priority = members
            .iter()
            .map(|&k| all[k].series.series_priority.clone())
            .min()
            .unwrap_or_else(|| "A".into());
        let no_valid_in_group = members.iter().all(|&k| all[k].valid_doses() == 0);
        let default = members
            .iter()
            .copied()
            .find(|&k| all[k].series.default_series);
        // SELECTSCORE-2
        let scorable: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&k| {
                let ps = &all[k];
                match ps.series.series_type {
                    SeriesType::Risk => ps.series.series_priority <= top_priority && candidate(k),
                    SeriesType::Standard => {
                        let with_valid = ps.valid_doses() > 0 && {
                            let first_valid = (0..doses.len())
                                .find(|&j| ps.status_of(j) == Some(EvalStatus::Valid))
                                .map(|j| doses[j].date);
                            match (first_valid, self.age_date(ps.series.max_age_to_start)) {
                                (Some(d), Some(max)) => d < max,
                                _ => true,
                            }
                        };
                        (with_valid || (no_valid_in_group && default.is_none())) && candidate(k)
                    }
                    SeriesType::EvaluationOnly => ps.status == SeriesStatus::Complete,
                }
            })
            .collect();
        let complete: Vec<usize> = scorable
            .iter()
            .copied()
            .filter(|&k| all[k].status == SeriesStatus::Complete)
            .collect();
        let in_process: Vec<usize> = scorable
            .iter()
            .copied()
            .filter(|&k| all[k].valid_doses() > 0 && all[k].status == SeriesStatus::NotComplete)
            .collect();
        // Table 8-3
        match scorable.len() {
            0 => return default,
            1 => return Some(scorable[0]),
            _ => {}
        }
        if complete.len() == 1 {
            return Some(complete[0]);
        }
        if complete.is_empty() && in_process.len() == 1 {
            return Some(in_process[0]);
        }
        if complete.is_empty()
            && in_process.is_empty()
            && let Some(d) = default
        {
            return Some(d);
        }
        // Table 8-5
        let (scored, kind): (Vec<usize>, u8) = if complete.len() >= 2 {
            (complete, 0)
        } else if in_process.len() >= 2 {
            (in_process, 1)
        } else {
            // All scorable series have no valid doses (Table 8-5, rule 3).
            // Mixed cases the table does not cover are scored the same way.
            (scorable.clone(), 2)
        };
        let scores = self.score(all, doses, &scored, kind);
        // SELECTBEST-1, SELECTBEST-2
        let best_score = scores.iter().copied().max()?;
        scored
            .iter()
            .zip(scores.iter())
            .filter(|(_, s)| **s == best_score)
            .map(|(k, _)| *k)
            .min_by_key(|&k| all[k].series.series_preference.unwrap_or(u32::MAX))
    }

    /// Points awarded under Tables 8-7, 8-9, and 8-11.
    fn score(
        &self,
        all: &[PatientSeries],
        doses: &[AntigenDose],
        scored: &[usize],
        kind: u8,
    ) -> Vec<i32> {
        let n = scored.len();
        let mut points = vec![0i32; n];
        // Award +win / 0 tie / -lose for a property computed per series.
        let award = |points: &mut [i32], flags: &[bool], win: i32, tie: Option<i32>, lose: i32| {
            let count = flags.iter().filter(|&&f| f).count();
            for (p, &f) in points.iter_mut().zip(flags) {
                *p += if !f {
                    lose
                } else if count >= 2 {
                    tie.unwrap_or(win)
                } else {
                    win
                };
            }
        };
        let valid: Vec<usize> = scored.iter().map(|&k| all[k].valid_doses()).collect();
        let max_valid = valid.iter().copied().max().unwrap_or(0);
        let most_valid: Vec<bool> = valid.iter().map(|&v| v >= max_valid).collect();
        let finish: Vec<Option<NaiveDate>> =
            scored.iter().map(|&k| self.finish_date(&all[k])).collect();
        let completable: Vec<bool> = scored
            .iter()
            .zip(&finish)
            .map(|(&k, f)| self.completable(&all[k], *f))
            .collect();
        let product: Vec<bool> = scored.iter().map(|&k| all[k].series.product_path).collect();
        match kind {
            0 => award(&mut points, &most_valid, 1, Some(0), -1),
            1 => {
                let product_all_valid: Vec<bool> = scored
                    .iter()
                    .map(|&k| {
                        let ps = &all[k];
                        ps.series.product_path
                            && (0..doses.len()).all(|j| {
                                ps.evals[j].as_ref().is_none_or(|e| {
                                    e.target.is_none() || e.status == EvalStatus::Valid
                                })
                            })
                    })
                    .collect();
                award(&mut points, &product_all_valid, 2, None, -2);
                award(&mut points, &completable, 3, None, -3);
                award(&mut points, &most_valid, 2, Some(0), -2);
                let remaining: Vec<usize> = scored
                    .iter()
                    .map(|&k| {
                        all[k]
                            .targets
                            .iter()
                            .filter(|t| t.status == TargetStatus::NotSatisfied)
                            .count()
                    })
                    .collect();
                let min_rem = remaining.iter().copied().min().unwrap_or(0);
                let closest: Vec<bool> = remaining.iter().map(|&r| r <= min_rem).collect();
                award(&mut points, &closest, 2, Some(0), -2);
                let min_finish = finish
                    .iter()
                    .zip(&completable)
                    .filter(|(_, c)| **c)
                    .filter_map(|(f, _)| *f)
                    .min();
                let earliest_finish: Vec<bool> = finish
                    .iter()
                    .zip(&completable)
                    .map(|(f, c)| *c && f.is_some() && *f <= min_finish)
                    .collect();
                award(&mut points, &earliest_finish, 1, Some(0), -1);
            }
            _ => {
                let starts: Vec<Option<NaiveDate>> =
                    scored.iter().map(|&k| all[k].dates.earliest).collect();
                let min_start = starts.iter().flatten().min().copied();
                let start_earliest: Vec<bool> = starts
                    .iter()
                    .map(|s| s.is_some() && *s <= min_start)
                    .collect();
                award(&mut points, &start_earliest, 1, Some(0), -1);
                award(&mut points, &completable, 1, None, -1);
                let not_product: Vec<bool> = product.iter().map(|p| !p).collect();
                award(&mut points, &not_product, 1, None, -1);
            }
        }
        points
    }

    /// SELECTB-12: earliest date plus the minimum intervals of the remaining
    /// target doses.
    fn finish_date(&self, ps: &PatientSeries) -> Option<NaiveDate> {
        let mut d = ps.dates.earliest?;
        for t in ps.current + 1..ps.targets.len() {
            let sdose = ps.dose(t);
            if let Some(iv) = sdose
                .intervals
                .iter()
                .find(|iv| iv.from_previous && iv.effective.applies(self.assess))
                && let Some(next) = add(iv.min_int, d)
            {
                d = next;
            }
            if let Some(a) = self.age_spec(sdose, self.assess)
                && let Some(min) = self.age_date(a.min_age)
            {
                d = d.max(min);
            }
        }
        Some(d)
    }

    /// SELECTB-3.
    fn completable(&self, ps: &PatientSeries, finish: Option<NaiveDate>) -> bool {
        let Some(finish) = finish else {
            return false;
        };
        let last = ps.series.doses.last();
        match last
            .and_then(|d| self.age_spec(d, self.assess))
            .and_then(|a| self.age_date(a.max_age))
        {
            Some(max) => finish < max,
            None => true,
        }
    }

    // ---------------------------------------------------- results

    fn series_result(
        &self,
        ps: &PatientSeries,
        doses: &[AntigenDose],
        best: bool,
    ) -> PatientSeriesResult {
        let evaluations = doses
            .iter()
            .enumerate()
            .filter_map(|(j, d)| {
                let e = ps.evals[j].as_ref()?;
                Some(DoseEvaluation {
                    dose_index: d.input,
                    date: d.date,
                    cvx: d.cvx.clone(),
                    antigen: ps.antigen.name.clone(),
                    status: e.status,
                    target_dose: e.target.map(|t| t + 1),
                    reasons: e.reasons.clone(),
                })
            })
            .collect();
        let remaining_doses = if ps.status == SeriesStatus::NotComplete {
            (ps.current..ps.targets.len())
                .filter(|&t| ps.targets[t].status == TargetStatus::NotSatisfied)
                .map(|t| {
                    let sdose = ps.dose(t);
                    let age = self.age_spec(sdose, self.assess);
                    RemainingDose {
                        dose_number: t + 1,
                        min_interval: sdose
                            .intervals
                            .iter()
                            .find(|iv| iv.from_previous && iv.effective.applies(self.assess))
                            .and_then(|iv| iv.min_int)
                            .map(|s| s.describe()),
                        min_age: age.and_then(|a| a.min_age).map(|s| s.describe()),
                        recommended_age: age.and_then(|a| a.earliest_rec_age).map(|s| s.describe()),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        // FORECASTPRIORITY-1
        let priority_forecast =
            ps.status == SeriesStatus::NotComplete && ps.current < ps.targets.len() && {
                let ivs: Vec<_> = ps
                    .dose(ps.current)
                    .intervals
                    .iter()
                    .filter(|iv| iv.effective.applies(self.assess))
                    .collect();
                !ivs.is_empty() && ivs.iter().all(|iv| iv.priority)
            };
        // Preferable vaccines for the forecast target dose that fit the
        // patient's age on the earliest date and are not contraindicated.
        let candidate_vaccines = match (ps.status, ps.dates.earliest) {
            (SeriesStatus::NotComplete, Some(earliest)) if ps.current < ps.targets.len() => {
                let ci = self.contraindicated_cvx(ps.antigen);
                let mut seen = BTreeSet::new();
                ps.dose(ps.current)
                    .preferable_vaccines
                    .iter()
                    .filter(|p| !ci.contains(&p.cvx))
                    .filter(|p| {
                        let begin = self.age_date(p.begin_age).unwrap_or_else(min_date);
                        let end = self.age_date(p.end_age).unwrap_or_else(max_date);
                        begin <= earliest && earliest < end
                    })
                    .filter(|p| seen.insert(p.cvx.clone()))
                    .map(|p| RecommendedVaccine {
                        cvx: p.cvx.clone(),
                        vaccine_type: p.vaccine_type.clone(),
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        PatientSeriesResult {
            priority_forecast,
            candidate_vaccines,
            antigen: ps.antigen.name.clone(),
            series_name: ps.series.name.clone(),
            series_type: ps.series.series_type.label().to_string(),
            series_group: ps.series.series_group.clone(),
            status: ps.status,
            forecast_reasons: ps.reasons.clone(),
            forecast_dose_number: ps.forecast_dose_number,
            dates: ps.dates.clone(),
            recommended_vaccines: ps.recommended.clone(),
            valid_doses: ps.valid_doses(),
            target_doses: ps.targets.len(),
            evaluations,
            remaining_doses,
            admin_guidance: ps.series.admin_guidance.clone(),
            best,
        }
    }

    // ------------------------------------------------ 9 vaccine groups

    fn vaccine_groups(
        &self,
        series: &[PatientSeriesResult],
        best_by_antigen: &BTreeMap<String, Vec<usize>>,
    ) -> Vec<VaccineGroupForecast> {
        let mut out = Vec::new();
        for vg in &self.sd.schedule.vaccine_groups {
            // FORECASTVG-1: best series of the group's antigens, by series group.
            let mut by_group: BTreeMap<(u32, String), Vec<usize>> = BTreeMap::new();
            for antigen in &vg.antigens {
                for &k in best_by_antigen.get(antigen).into_iter().flatten() {
                    by_group
                        .entry(group_key(&series[k].series_group))
                        .or_default()
                        .push(k);
                }
            }
            for ((_, group), members) in by_group {
                let forecasts: Vec<&PatientSeriesResult> =
                    members.iter().map(|&k| &series[k]).collect();
                let multi = vg.antigens.len() > 1;
                let status = if !multi {
                    forecasts[0].status
                } else {
                    let any = |s: SeriesStatus| forecasts.iter().any(|f| f.status == s);
                    // Table 9-4
                    if any(SeriesStatus::Contraindicated) {
                        SeriesStatus::Contraindicated
                    } else if any(SeriesStatus::AgedOut) {
                        SeriesStatus::AgedOut
                    } else if any(SeriesStatus::NotRecommended) {
                        SeriesStatus::NotRecommended
                    } else if any(SeriesStatus::NotComplete) {
                        SeriesStatus::NotComplete
                    } else if forecasts.iter().all(|f| f.status == SeriesStatus::Immune) {
                        SeriesStatus::Immune
                    } else {
                        SeriesStatus::Complete
                    }
                };
                let open: Vec<&PatientSeriesResult> = forecasts
                    .iter()
                    .copied()
                    .filter(|f| f.status == SeriesStatus::NotComplete)
                    .collect();
                let mut dates = ForecastDates::default();
                let mut dose_number = None;
                if status == SeriesStatus::NotComplete && !open.is_empty() {
                    let earliest_all: Vec<NaiveDate> =
                        open.iter().filter_map(|f| f.dates.earliest).collect();
                    let earliest = if !multi {
                        earliest_all.iter().min().copied()
                    } else {
                        // MULTIANTVG-1, FORECASTPRIORITY-1
                        let priority = open.iter().any(|f| f.priority_forecast);
                        if priority {
                            let last_given = self
                                .p
                                .doses
                                .iter()
                                .filter(|d| {
                                    self.sd
                                        .schedule
                                        .cvx_map
                                        .get(&normalize_cvx(&d.cvx))
                                        .is_some_and(|e| {
                                            e.associations
                                                .iter()
                                                .any(|a| vg.antigens.contains(&a.antigen))
                                        })
                                })
                                .map(|d| d.date)
                                .max();
                            earliest_all
                                .iter()
                                .min()
                                .copied()
                                .map(|e| match last_given {
                                    Some(l) => e.max(l),
                                    None => e,
                                })
                        } else {
                            earliest_all.iter().max().copied()
                        }
                    };
                    let min_of = |f: fn(&ForecastDates) -> Option<NaiveDate>| {
                        open.iter().filter_map(|s| f(&s.dates)).min()
                    };
                    let rec = min_of(|d| d.recommended);
                    let past = min_of(|d| d.past_due);
                    dates = ForecastDates {
                        earliest,
                        recommended: rec.map(|r| earliest.map_or(r, |e| r.max(e))), // VG-2
                        past_due: past.map(|p| earliest.map_or(p, |e| p.max(e))),   // VG-3
                        latest: min_of(|d| d.latest),                               // VG-4
                        unadjusted_recommended: min_of(|d| d.unadjusted_recommended), // VG-5
                        unadjusted_past_due: min_of(|d| d.unadjusted_past_due),     // VG-6
                    };
                    // FORECASTDN-2
                    let numbers = open.iter().filter_map(|f| f.forecast_dose_number);
                    dose_number = if vg.administer_full_vaccine_group {
                        numbers.min()
                    } else {
                        numbers.max()
                    };
                }
                let mut reasons: Vec<String> = Vec::new();
                for f in &forecasts {
                    for r in &f.forecast_reasons {
                        let r = if multi {
                            format!("{}: {r}", f.antigen)
                        } else {
                            r.clone()
                        };
                        if !reasons.contains(&r) {
                            reasons.push(r);
                        }
                    }
                }
                let mut recommended_vaccines: Vec<RecommendedVaccine> = Vec::new();
                for f in &open {
                    for v in &f.recommended_vaccines {
                        if !recommended_vaccines.contains(v) {
                            recommended_vaccines.push(v.clone());
                        }
                    }
                }
                let series_type = forecasts
                    .iter()
                    .map(|f| f.series_type.as_str())
                    .find(|t| *t == "Risk")
                    .unwrap_or(forecasts[0].series_type.as_str())
                    .to_string();
                out.push(VaccineGroupForecast {
                    vaccine_group: vg.name.clone(),
                    series_group: group,
                    series_type,
                    status,
                    forecast_dose_number: dose_number,
                    dates,
                    forecast_reasons: reasons,
                    recommended_antigens: open.iter().map(|f| f.antigen.clone()).collect(),
                    recommended_vaccines,
                    patient_series: members,
                });
            }
        }
        out
    }
}
