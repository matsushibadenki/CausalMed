//! A deliberately small M0 pipeline. This measures a baseline, not causal discovery.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sample {
    pub id: String,
    pub dataset: String,
    pub donor: String,
    pub perturbation: String,
    pub condition: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkReport {
    pub id: String,
    pub created_at: String,
    pub model: String,
    pub synthetic: bool,
    pub dataset_sha256: String,
    pub held_out_dataset: String,
    pub held_out_perturbation: String,
    pub train_ids: Vec<String>,
    pub test_ids: Vec<String>,
    pub train_donors: usize,
    pub test_donors: usize,
    pub control_mean: f64,
    pub donor_mae: f64,
    pub abstained: bool,
    pub reason: Option<String>,
}

pub fn evaluate(
    samples: &[Sample],
    dataset: &str,
    perturbation: &str,
    checksum: String,
) -> Result<BenchmarkReport> {
    let mut ids = BTreeSet::new();
    if samples.is_empty()
        || samples.iter().any(|s| {
            !s.value.is_finite() || s.id.is_empty() || s.donor.is_empty() || !ids.insert(&s.id)
        })
    {
        return Err(Error::Validation("invalid_samples".into()));
    }
    // Global donor IDs must be unique across datasets; callers normalize these upstream.
    let test: Vec<_> = samples
        .iter()
        .filter(|s| s.dataset == dataset && s.perturbation == perturbation)
        .collect();
    let test_donors: BTreeSet<_> = test.iter().map(|s| &s.donor).collect();
    let train: Vec<_> = samples
        .iter()
        .filter(|s| {
            s.dataset != dataset
                && s.perturbation != perturbation
                && !test_donors.contains(&s.donor)
        })
        .collect();
    let conditions: BTreeSet<_> = test.iter().map(|s| &s.condition).collect();
    let controls: Vec<_> = train
        .iter()
        .copied()
        .filter(|s| s.perturbation == "control" && conditions.contains(&s.condition))
        .collect();
    let means = |rows: &[&Sample]| {
        let mut donors: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
        for s in rows {
            let entry = donors.entry(&s.donor).or_default();
            entry.0 += s.value;
            entry.1 += 1;
        }
        donors
            .values()
            .map(|(sum, n)| sum / *n as f64)
            .collect::<Vec<_>>()
    };
    let control_means = means(&controls);
    let test_means = means(&test);
    let reason = if conditions.len() != 1 {
        Some("single_condition_required")
    } else if control_means.len() < 2 || test_means.len() < 2 {
        Some("insufficient_independent_donors")
    } else {
        None
    };
    let control_mean = if control_means.is_empty() {
        0.0
    } else {
        control_means.iter().sum::<f64>() / control_means.len() as f64
    };
    let donor_mae = if reason.is_some() {
        0.0
    } else {
        test_means
            .iter()
            .map(|v| (v - control_mean).abs())
            .sum::<f64>()
            / test_means.len() as f64
    };
    Ok(BenchmarkReport {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        model: "M0-control-mean-v1".into(),
        synthetic: true,
        dataset_sha256: checksum,
        held_out_dataset: dataset.into(),
        held_out_perturbation: perturbation.into(),
        train_ids: train.iter().map(|s| s.id.clone()).collect(),
        test_ids: test.iter().map(|s| s.id.clone()).collect(),
        train_donors: control_means.len(),
        test_donors: test_means.len(),
        control_mean,
        donor_mae,
        abstained: reason.is_some(),
        reason: reason.map(str::to_owned),
    })
}
