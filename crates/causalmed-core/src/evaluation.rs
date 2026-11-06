//! Scalar count-outcome inspection and conservative donor-level M0. No causal claim.
use crate::{
    cohorts::{CohortReview, Eligibility},
    invalid, DatasetMetadata, Document, ResearchStore, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_LOG: f64 = 9.210440366976517; // log(1 + 10,000)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutcomeSpec {
    pub feature_id: String,
    pub feature_column: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationPlan {
    pub spec: OutcomeSpec,
    pub measurement_evidence: String,
    pub reviewed_for: Option<String>,
}
impl Default for EvaluationPlan {
    fn default() -> Self {
        Self {
            spec: OutcomeSpec {
                feature_id: String::new(),
                feature_column: "_index".into(),
            },
            measurement_evidence: String::new(),
            reviewed_for: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationInput {
    pub source_sha256: String,
    pub source_name: String,
    pub metadata: DatasetMetadata,
    pub cohort: CohortReview,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationContext {
    pub version: u32,
    pub spec: OutcomeSpec,
    pub inputs: Vec<EvaluationInput>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DonorOutcome {
    pub label: String,
    pub cells: u64,
    pub mean: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceOutcome {
    pub source_sha256: String,
    pub qc_id: String,
    pub matrix: String,
    pub feature_count: u64,
    pub feature_space_sha256: Option<String>,
    pub matched_features: u64,
    pub retained: u64,
    pub missing_donor_cells: u64,
    pub issues: Vec<String>,
    pub donors: Vec<DonorOutcome>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutcomeResult {
    pub version: u32,
    pub context_sha256: String,
    pub inputs: Vec<SourceOutcome>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationKind {
    Inspection,
    Baseline,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationReason {
    pub code: String,
    pub source_sha256: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationRecord {
    pub id: String,
    pub created_at: String,
    pub kind: EvaluationKind,
    pub context: EvaluationContext,
    pub context_sha256: String,
    pub inspection_id: Option<String>,
    pub outcome: Option<OutcomeResult>,
    pub eligibility: Option<Eligibility>,
    pub measurement_evidence: String,
    pub reviewed_for: Option<String>,
    pub abstained: bool,
    pub reasons: Vec<EvaluationReason>,
    pub control_mean: Option<f64>,
    pub mae: Option<f64>,
    pub train_donors: usize,
    pub test_donors: usize,
}
fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn text(s: &str, n: usize) -> bool {
    s.len() <= n && !s.contains('\0')
}
fn spec_valid(s: &OutcomeSpec, allow_empty: bool) -> bool {
    text(&s.feature_id, 200)
        && (allow_empty || !s.feature_id.trim().is_empty())
        && !s.feature_column.trim().is_empty()
        && text(&s.feature_column, 200)
}
pub fn context_sha(ctx: &EvaluationContext) -> String {
    crate::checksum(&serde_json::to_vec(ctx).expect("evaluation context serialization"))
}
pub fn context(doc: &Document) -> Result<EvaluationContext> {
    if !spec_valid(&doc.evaluation_plan.spec, false) {
        return Err(invalid("invalid_outcome_spec"));
    }
    let mut inputs = vec![];
    for cohort in &doc.cohorts {
        if cohort.settings.partition == "exclude" {
            continue;
        }
        let dataset = doc
            .datasets
            .iter()
            .find(|d| d.observation.sha256 == cohort.observation.source_sha256)
            .ok_or_else(|| invalid("dataset_missing"))?;
        inputs.push(EvaluationInput {
            source_sha256: dataset.observation.sha256.clone(),
            source_name: dataset.metadata.name.clone(),
            metadata: dataset.metadata.clone(),
            cohort: cohort.clone(),
        });
    }
    inputs.sort_by(|a, b| a.source_sha256.cmp(&b.source_sha256));
    let ctx = EvaluationContext {
        version: 1,
        spec: doc.evaluation_plan.spec.clone(),
        inputs,
    };
    validate_context(&ctx)?;
    Ok(ctx)
}
fn validate_context(ctx: &EvaluationContext) -> Result<()> {
    if ctx.version != 1
        || !spec_valid(&ctx.spec, false)
        || ctx.inputs.len() > 32
        || serde_json::to_vec(ctx)?.len() > 96 * 1024
    {
        return Err(invalid("evaluation_context_too_large"));
    }
    let mut ids = BTreeSet::new();
    for input in &ctx.inputs {
        if !sha(&input.source_sha256)
            || !ids.insert(&input.source_sha256)
            || !text(&input.source_name, 200)
            || input.source_sha256 != input.cohort.observation.source_sha256
            || input.cohort.observation.donors.len() > 30
            || input.cohort.settings.mappings.len() > 30
        {
            return Err(invalid("invalid_outcome_result"));
        }
    }
    Ok(())
}
pub fn validate_outcome(ctx: &EvaluationContext, result: &OutcomeResult) -> Result<()> {
    validate_context(ctx)?;
    if result.version != 1
        || result.context_sha256 != context_sha(ctx)
        || result.inputs.len() != ctx.inputs.len()
    {
        return Err(invalid("invalid_outcome_result"));
    }
    let mut seen = BTreeSet::new();
    for row in &result.inputs {
        let source = ctx
            .inputs
            .iter()
            .find(|s| s.source_sha256 == row.source_sha256)
            .ok_or_else(|| invalid("invalid_outcome_result"))?;
        let obs = &source.cohort.observation;
        if !seen.insert(&row.source_sha256)
            || row.qc_id != obs.qc_id
            || row.retained != obs.retained
            || row.missing_donor_cells != obs.missing_retained
            || !text(&row.matrix, 200)
            || row.donors.len() > 30
            || row.issues.len() > 10
            || row.feature_space_sha256.as_ref().is_some_and(|s| !sha(s))
            || row.issues.iter().any(|s| {
                ![
                    "feature_missing",
                    "feature_ambiguous",
                    "feature_space_invalid",
                    "matrix_unavailable",
                    "invalid_expression",
                    "invalid_library",
                    "missing_donors",
                    "empty_cohort",
                    "selection_changed",
                    "file_changed",
                ]
                .contains(&s.as_str())
            })
        {
            return Err(invalid("invalid_outcome_result"));
        }
        if !row.issues.is_empty() {
            if !row.donors.is_empty() {
                return Err(invalid("invalid_outcome_result"));
            }
            continue;
        }
        if row.feature_count == 0
            || row.feature_space_sha256.is_none()
            || row.matched_features != 1
            || row.missing_donor_cells != 0
            || row.retained == 0
        {
            return Err(invalid("invalid_outcome_result"));
        }
        let active: Vec<_> = obs.donors.iter().filter(|d| d.retained > 0).collect();
        if row.donors.len() != active.len() {
            return Err(invalid("invalid_outcome_result"));
        }
        let mut labels = BTreeSet::new();
        for d in &row.donors {
            if !labels.insert(&d.label)
                || !active
                    .iter()
                    .any(|a| a.label == d.label && a.retained == d.cells)
                || !d.mean.is_finite()
                || d.mean < 0.0
                || d.mean > MAX_LOG + 1e-8
            {
                return Err(invalid("invalid_outcome_result"));
            }
        }
    }
    Ok(())
}
fn reason(code: &str, source: Option<String>) -> EvaluationReason {
    EvaluationReason {
        code: code.into(),
        source_sha256: source,
    }
}
fn calculate(
    ctx: &EvaluationContext,
    result: &OutcomeResult,
) -> (Vec<EvaluationReason>, Option<(f64, f64, usize, usize)>) {
    let mut issues = vec![];
    let spaces: BTreeSet<_> = result
        .inputs
        .iter()
        .filter_map(|r| r.feature_space_sha256.as_deref())
        .collect();
    let sizes: BTreeSet<_> = result
        .inputs
        .iter()
        .filter(|r| r.issues.is_empty())
        .map(|r| r.feature_count)
        .collect();
    let matrices: BTreeSet<_> = result
        .inputs
        .iter()
        .filter(|r| r.issues.is_empty())
        .map(|r| r.matrix.as_str())
        .collect();
    if spaces.len() != 1 || sizes.len() > 1 {
        issues.push(reason("feature_space_mismatch", None));
    }
    if matrices.len() != 1 {
        issues.push(reason("matrix_mismatch", None));
    }
    let mut train = BTreeMap::new();
    let mut test = BTreeMap::new();
    for input in &ctx.inputs {
        let Some(row) = result
            .inputs
            .iter()
            .find(|r| r.source_sha256 == input.source_sha256)
        else {
            issues.push(reason("inspection_missing", None));
            continue;
        };
        for code in &row.issues {
            issues.push(reason(code, Some(row.source_sha256.clone())));
        }
        let settings = &input.cohort.settings;
        for donor in &row.donors {
            let Some(mapping) = settings
                .mappings
                .iter()
                .find(|m| m.label == donor.label && m.reviewed && !m.canonical_id.is_empty())
            else {
                issues.push(reason("donors_unreviewed", Some(row.source_sha256.clone())));
                continue;
            };
            let target = if settings.partition == "train" && settings.is_control {
                Some(&mut train)
            } else if settings.partition == "test" {
                Some(&mut test)
            } else {
                None
            };
            if let Some(target) = target {
                if target
                    .insert(mapping.canonical_id.clone(), donor.mean)
                    .is_some()
                {
                    issues.push(reason("donor_reused", Some(row.source_sha256.clone())));
                }
            }
        }
    }
    if train.keys().any(|d| test.contains_key(d)) {
        issues.push(reason("donor_overlap", None));
    }
    if train.len() < 2 || test.len() < 2 {
        issues.push(reason("insufficient_donors", None));
    }
    if !issues.is_empty() {
        return (issues, None);
    }
    let prediction = train.values().sum::<f64>() / train.len() as f64;
    let mae = test.values().map(|v| (v - prediction).abs()).sum::<f64>() / test.len() as f64;
    (issues, Some((prediction, mae, train.len(), test.len())))
}
pub(crate) fn validate(doc: &Document) -> Result<()> {
    let p = &doc.evaluation_plan;
    if !spec_valid(&p.spec, true)
        || !text(&p.measurement_evidence, 2000)
        || p.reviewed_for
            .as_ref()
            .is_some_and(|s| !sha(s) || p.measurement_evidence.trim().is_empty())
    {
        return Err(invalid("invalid_outcome_spec"));
    }
    if doc.evaluations.len() > 8 || serde_json::to_vec(&doc.evaluations)?.len() > 1024 * 1024 {
        return Err(invalid("evaluation_history_full"));
    }
    let mut ids = BTreeSet::new();
    for r in &doc.evaluations {
        validate_context(&r.context)?;
        if uuid::Uuid::parse_str(&r.id).is_err()
            || !ids.insert(&r.id)
            || chrono::DateTime::parse_from_rfc3339(&r.created_at).is_err()
            || r.context_sha256 != context_sha(&r.context)
            || r.reasons.len() > 256
            || r.reasons
                .iter()
                .any(|s| !text(&s.code, 80) || s.source_sha256.as_ref().is_some_and(|s| !sha(s)))
            || !text(&r.measurement_evidence, 2000)
            || r.reviewed_for.as_ref().is_some_and(|s| !sha(s))
        {
            return Err(invalid("invalid_outcome_result"));
        }
        match r.kind {
            EvaluationKind::Inspection => {
                let out = r
                    .outcome
                    .as_ref()
                    .ok_or_else(|| invalid("invalid_outcome_result"))?;
                validate_outcome(&r.context, out)?;
                if r.abstained != out.inputs.iter().any(|i| !i.issues.is_empty())
                    || r.control_mean.is_some()
                    || r.mae.is_some()
                    || r.eligibility.is_some()
                    || r.inspection_id.is_some()
                {
                    return Err(invalid("invalid_outcome_result"));
                }
            }
            EvaluationKind::Baseline => {
                if r.outcome.is_some() {
                    return Err(invalid("invalid_outcome_result"));
                }
                if r.abstained {
                    if r.mae.is_some() || r.control_mean.is_some() || r.reasons.is_empty() {
                        return Err(invalid("invalid_outcome_result"));
                    }
                } else {
                    let inspection = doc
                        .evaluations
                        .iter()
                        .find(|i| {
                            Some(&i.id) == r.inspection_id.as_ref()
                                && i.kind == EvaluationKind::Inspection
                                && i.context_sha256 == r.context_sha256
                        })
                        .ok_or_else(|| invalid("inspection_missing"))?;
                    let (_, score) = calculate(
                        &r.context,
                        inspection
                            .outcome
                            .as_ref()
                            .ok_or_else(|| invalid("inspection_missing"))?,
                    );
                    let (mean, mae, ntrain, ntest) =
                        score.ok_or_else(|| invalid("invalid_outcome_result"))?;
                    if r.reviewed_for.as_deref() != Some(&r.context_sha256)
                        || r.measurement_evidence.trim().is_empty()
                        || r.eligibility.as_ref().is_none_or(|e| {
                            !e.ready
                                || !e.issues.is_empty()
                                || e.cohorts
                                    .iter()
                                    .any(|c| c.participating && !c.issues.is_empty())
                        })
                        || !r.reasons.is_empty()
                        || r.control_mean != Some(mean)
                        || r.mae != Some(mae)
                        || r.train_donors != ntrain
                        || r.test_donors != ntest
                    {
                        return Err(invalid("invalid_outcome_result"));
                    }
                }
            }
        }
    }
    Ok(())
}
impl ResearchStore {
    pub fn set_evaluation_plan(
        &mut self,
        spec: OutcomeSpec,
        evidence: String,
        reviewed: bool,
        revision: u64,
    ) -> Result<Document> {
        if self.document.revision != revision {
            return Err(invalid("dataset_changed"));
        }
        if !spec_valid(&spec, false)
            || !text(&evidence, 2000)
            || (reviewed && evidence.trim().is_empty())
        {
            return Err(invalid("invalid_outcome_spec"));
        }
        let mut doc = self.snapshot();
        doc.evaluation_plan = EvaluationPlan {
            spec,
            measurement_evidence: evidence,
            reviewed_for: None,
        };
        if reviewed {
            let ctx = context(&doc)?;
            doc.evaluation_plan.reviewed_for = Some(context_sha(&ctx));
        }
        validate(&doc)?;
        self.commit(doc)
    }
    pub fn record_outcome(
        &mut self,
        ctx: EvaluationContext,
        outcome: OutcomeResult,
    ) -> Result<Document> {
        let mut doc = self.snapshot();
        if context_sha(&context(&doc)?) != context_sha(&ctx) {
            return Err(invalid("outcome_stale"));
        }
        validate_outcome(&ctx, &outcome)?;
        let reasons = outcome
            .inputs
            .iter()
            .flat_map(|s| {
                s.issues
                    .iter()
                    .map(|code| reason(code, Some(s.source_sha256.clone())))
            })
            .collect::<Vec<_>>();
        let record = EvaluationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            kind: EvaluationKind::Inspection,
            context_sha256: context_sha(&ctx),
            context: ctx,
            inspection_id: None,
            outcome: Some(outcome),
            eligibility: None,
            measurement_evidence: String::new(),
            reviewed_for: None,
            abstained: !reasons.is_empty(),
            reasons,
            control_mean: None,
            mae: None,
            train_donors: 0,
            test_donors: 0,
        };
        doc.evaluations.push(record);
        validate(&doc)?;
        self.commit(doc)
    }
    pub fn run_real_baseline(&mut self, eligibility: Eligibility) -> Result<Document> {
        let mut doc = self.snapshot();
        if eligibility.document_revision != doc.revision {
            return Err(invalid("outcome_stale"));
        }
        let ctx = context(&doc)?;
        let signature = context_sha(&ctx);
        let mut reasons = vec![];
        if !eligibility.ready
            || !eligibility.issues.is_empty()
            || eligibility
                .cohorts
                .iter()
                .any(|c| c.participating && !c.issues.is_empty())
        {
            reasons.push(reason("eligibility_blocked", None));
            for code in &eligibility.issues {
                reasons.push(reason(code, None));
            }
            for c in &eligibility.cohorts {
                if c.participating {
                    for code in &c.issues {
                        reasons.push(reason(code, Some(c.source_sha256.clone())));
                    }
                }
            }
        }
        if doc.evaluation_plan.reviewed_for.as_deref() != Some(&signature) {
            reasons.push(reason("measurement_unreviewed", None));
        }
        let inspection = doc
            .evaluations
            .iter()
            .rev()
            .find(|r| r.kind == EvaluationKind::Inspection && r.context_sha256 == signature);
        let mut values = None;
        if let Some(inspection) = inspection {
            let (issues, score) = calculate(
                &ctx,
                inspection
                    .outcome
                    .as_ref()
                    .ok_or_else(|| invalid("invalid_outcome_result"))?,
            );
            reasons.extend(issues);
            if reasons.is_empty() {
                values = score;
            }
        } else {
            reasons.push(reason("inspection_missing", None));
        }
        let mut seen = BTreeSet::new();
        reasons.retain(|r| seen.insert((r.code.clone(), r.source_sha256.clone())));
        let (control_mean, mae, ntrain, ntest) =
            values.map_or((None, None, 0, 0), |(a, b, c, d)| (Some(a), Some(b), c, d));
        let record = EvaluationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            kind: EvaluationKind::Baseline,
            context_sha256: signature,
            context: ctx,
            inspection_id: inspection.map(|i| i.id.clone()),
            outcome: None,
            eligibility: Some(eligibility),
            measurement_evidence: doc.evaluation_plan.measurement_evidence.clone(),
            reviewed_for: doc.evaluation_plan.reviewed_for.clone(),
            abstained: values.is_none(),
            reasons,
            control_mean,
            mae,
            train_donors: ntrain,
            test_donors: ntest,
        };
        doc.evaluations.push(record);
        validate(&doc)?;
        self.commit(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> ResearchStore {
        let mut s = crate::cohorts::tests::store();
        s.set_evaluation_plan(
            OutcomeSpec {
                feature_id: "QA-G".into(),
                feature_column: "_index".into(),
            },
            "Synthetic measurement comparison only".into(),
            true,
            s.snapshot().revision,
        )
        .unwrap();
        s
    }
    fn output(ctx: &EvaluationContext) -> OutcomeResult {
        OutcomeResult {
            version: 1,
            context_sha256: context_sha(ctx),
            inputs: ctx
                .inputs
                .iter()
                .enumerate()
                .map(|(i, s)| SourceOutcome {
                    source_sha256: s.source_sha256.clone(),
                    qc_id: s.cohort.observation.qc_id.clone(),
                    matrix: "X".into(),
                    feature_count: 5,
                    feature_space_sha256: Some("e".repeat(64)),
                    matched_features: 1,
                    retained: 10,
                    missing_donor_cells: 0,
                    issues: vec![],
                    donors: s
                        .cohort
                        .observation
                        .donors
                        .iter()
                        .enumerate()
                        .map(|(j, d)| DonorOutcome {
                            label: d.label.clone(),
                            cells: d.retained,
                            mean: (i * 2 + j + 1) as f64,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
    fn eligibility(s: &ResearchStore) -> Eligibility {
        crate::cohorts::assess(
            &s.snapshot(),
            &[
                ("a".repeat(64), (true, true)),
                ("b".repeat(64), (true, true)),
            ]
            .into(),
        )
    }
    #[test]
    fn scalar_m0_equal_donor_weight_and_portable_audit() {
        let mut s = setup();
        let ctx = context(&s.snapshot()).unwrap();
        s.record_outcome(ctx.clone(), output(&ctx)).unwrap();
        let d = s.run_real_baseline(eligibility(&s)).unwrap();
        let r = d.evaluations.last().unwrap();
        assert!(!r.abstained);
        assert_eq!(r.control_mean, Some(1.5));
        assert_eq!(r.mae, Some(2.0));
        let mut restored = ResearchStore::in_memory().unwrap();
        restored.restore_document(d.clone(), 0).unwrap();
        assert_eq!(restored.snapshot().evaluations.len(), 2);
        let mut forged = d;
        forged.evaluations[1].mae = Some(0.0);
        assert!(restored.restore_document(forged, 0).is_err());
        assert!(ResearchStore::in_memory()
            .unwrap()
            .snapshot()
            .evaluations
            .is_empty());
    }
    #[test]
    fn mismatch_stale_and_unreviewed_abstain_without_scores() {
        let mut s = setup();
        let ctx = context(&s.snapshot()).unwrap();
        let mut out = output(&ctx);
        out.inputs[1].feature_space_sha256 = Some("d".repeat(64));
        s.record_outcome(ctx, out).unwrap();
        let d = s.run_real_baseline(eligibility(&s)).unwrap();
        assert!(d.evaluations.last().unwrap().abstained);
        assert!(d.evaluations.last().unwrap().mae.is_none());
        assert!(d
            .evaluations
            .last()
            .unwrap()
            .reasons
            .iter()
            .any(|r| r.code == "feature_space_mismatch"));
        let old = context(&s.snapshot()).unwrap();
        s.set_evaluation_plan(
            OutcomeSpec {
                feature_id: "other".into(),
                feature_column: "_index".into(),
            },
            "".into(),
            false,
            d.revision,
        )
        .unwrap();
        assert!(s.record_outcome(old.clone(), output(&old)).is_err());
        let d = s.run_real_baseline(eligibility(&s)).unwrap();
        assert!(d
            .evaluations
            .last()
            .unwrap()
            .reasons
            .iter()
            .any(|r| r.code == "measurement_unreviewed"));
        assert!(d
            .evaluations
            .last()
            .unwrap()
            .reasons
            .iter()
            .any(|r| r.code == "inspection_missing"));
    }
    #[test]
    fn malformed_values_and_history_limit_are_atomic() {
        let mut s = setup();
        let ctx = context(&s.snapshot()).unwrap();
        let mut out = output(&ctx);
        out.inputs[0].donors[0].mean = f64::NAN;
        assert!(s.record_outcome(ctx.clone(), out).is_err());
        for _ in 0..8 {
            s.record_outcome(ctx.clone(), output(&ctx)).unwrap();
        }
        let rev = s.snapshot().revision;
        assert!(s.record_outcome(ctx.clone(), output(&ctx)).is_err());
        assert_eq!(s.snapshot().revision, rev);
    }
    #[test]
    fn cell_replication_does_not_weight_donors_and_repeated_identity_abstains() {
        let s = setup();
        let mut ctx = context(&s.snapshot()).unwrap();
        let mut out = output(&ctx);
        let first = calculate(&ctx, &out).1.unwrap();
        // More cells for the same donor must never become more independent samples.
        out.inputs[0].donors[0].cells = 5000;
        assert_eq!(calculate(&ctx, &out).1.unwrap(), first);
        ctx.inputs[1].cohort.settings.partition = "train".into();
        ctx.inputs[1].cohort.settings.is_control = true;
        ctx.inputs[1].cohort.settings.mappings[0].canonical_id =
            ctx.inputs[0].cohort.settings.mappings[0]
                .canonical_id
                .clone();
        let (issues, score) = calculate(&ctx, &out);
        assert!(score.is_none());
        assert!(issues.iter().any(|r| r.code == "donor_reused"));
    }
}
