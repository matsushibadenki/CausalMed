use crate::h5ad::Worker;
use causalmed_core::{
    evaluation::{self, EvaluationContext},
    Document,
};
use serde_json::{json, Value};
pub fn jobs(doc: &Document, worker: &Worker) -> Result<(EvaluationContext, Value), String> {
    let ctx = evaluation::context(doc).map_err(|e| e.to_string())?;
    if ctx.inputs.is_empty() {
        return Err("evaluation_no_cohorts".into());
    }
    let jobs: Vec<_> = ctx
        .inputs
        .iter()
        .map(|i| {
            let o = &i.cohort.observation;
            // A trashed QC run cannot be used even if an old inspection is saved.
            if worker.history_record(&o.qc_id).is_err() {
                return json!({"issue":"selection_changed"});
            }
            match worker.export_source(&o.qc_id) {
                Ok(s)
                    if s.source.is_file() && s.selection["manifestSha256"] == o.manifest_sha256 =>
                {
                    json!({"path":s.source,"selectionDir":s.directory,"selection":s.selection})
                }
                _ => json!({"issue":"file_changed"}),
            }
        })
        .collect();
    Ok((ctx, json!(jobs)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cohorts, datasets, h5ad, project};
    use causalmed_core::{
        evaluation::{OutcomeResult, OutcomeSpec},
        ResearchStore,
    };
    use std::fs;
    #[test]
    #[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
    fn public_scalar_outcome_restore_relink_abstention_and_project_isolation() {
        let root = tempfile::tempdir().unwrap();
        let a = project::create(root.path(), "Real outcome A QA", 0).unwrap();
        let b = project::create(root.path(), "Real outcome B QA", 0).unwrap();
        let source = root.path().join("kang.h5ad");
        fs::copy(std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap(), &source).unwrap();
        let sha = project::digest(&source).unwrap();
        let w = Worker::new(a.path());
        let scope = w.snapshot().project_key;
        let mut s = ResearchStore::open(a.path().join("research.sqlite3")).unwrap();
        w.reserve(120).unwrap();
        w.run_analysis(
            &h5ad::python_path(),
            h5ad::worker_script(),
            &source,
            &json!({"donorColumn":"replicate"}),
            "",
        );
        assert_eq!(w.snapshot().status, "completed");
        let original_report = w.snapshot().report;
        let d = datasets::register(&mut s, &w, &scope, &sha).unwrap();
        let mut m = d.datasets[0].metadata.clone();
        m.donor_column = "replicate".into();
        m.condition_column = "cell_type".into();
        m.perturbation_column = "label".into();
        let d = s.update_dataset(&sha, m, d.revision).unwrap();
        let d = cohorts::capture(&mut s, &w, &scope, &sha, d.revision).unwrap();
        s.set_evaluation_plan(
            OutcomeSpec {
                feature_id: "ISG15".into(),
                feature_column: "_index".into(),
            },
            "".into(),
            false,
            d.revision,
        )
        .unwrap();
        let (ctx, input) = jobs(&s.snapshot(), &w).unwrap();
        w.reserve(120).unwrap();
        w.run_outcome(ctx.clone(), input);
        assert_eq!(w.snapshot().status, "completed", "{:?}", w.snapshot().error);
        assert_eq!(w.snapshot().operation, "inspect_outcome");
        assert_eq!(w.snapshot().report, original_report);
        let out: OutcomeResult =
            serde_json::from_value(w.snapshot().outcome_report.unwrap()).unwrap();
        assert_eq!(out.inputs[0].donors.len(), 8);
        assert!(out.inputs[0].issues.is_empty());
        // Independent full-matrix reference is confined to this test process.
        let selection = w
            .export_source(&ctx.inputs[0].cohort.observation.qc_id)
            .unwrap();
        let reference=std::process::Command::new(h5ad::python_path()).args(["-c",r#"import anndata as ad,numpy as np,json,sys
x=ad.read_h5ad(sys.argv[1]);q=np.load(sys.argv[2],allow_pickle=False);k=q['retained'];g=x.var_names.get_loc('ISG15');v=np.log1p(x.X[:,g].toarray().ravel()[k]/np.asarray(x.X.sum(axis=1)).ravel()[k]*10000);d=x.obs['replicate'].astype(str).to_numpy()[k];print(json.dumps({a:float(v[d==a].mean()) for a in np.unique(d)}))"#]).arg(&source).arg(selection.directory.join("cells.npz")).output().unwrap();
        assert!(reference.status.success());
        let means: Value = serde_json::from_slice(&reference.stdout).unwrap();
        for donor in &out.inputs[0].donors {
            assert!((donor.mean - means[&donor.label].as_f64().unwrap()).abs() < 2e-6);
        }
        eprintln!(
            "public ISG15 retained={} donors={:?}",
            out.inputs[0].retained,
            out.inputs[0]
                .donors
                .iter()
                .map(|d| (&d.label, d.mean))
                .collect::<Vec<_>>()
        );
        s.record_outcome(ctx, out).unwrap();
        let eligibility = cohorts::assess(&s.snapshot(), &w, &scope).unwrap();
        let doc = s.run_real_baseline(eligibility).unwrap();
        let baseline = doc.evaluations.last().unwrap();
        assert!(baseline.abstained);
        assert!(baseline.mae.is_none());
        assert!(baseline
            .reasons
            .iter()
            .any(|r| r.code == "measurement_unreviewed"));
        assert!(baseline.reasons.iter().any(|r| r.code == "qc_partial"));
        // A saved inspection cannot bypass a subsequently trashed QC run.
        let qid = &doc.cohorts[0].observation.qc_id;
        crate::trash::history(&w, &scope, qid, false).unwrap();
        let blocked = cohorts::assess(&s.snapshot(), &w, &scope).unwrap();
        assert!(blocked.cohorts[0].issues.contains(&"qc_unavailable".into()));
        let doc = s.run_real_baseline(blocked).unwrap();
        assert!(doc.evaluations.last().unwrap().mae.is_none());
        crate::trash::history(&w, &scope, qid, true).unwrap();
        let stage = project::stage_save(a.path(), &doc, &w).unwrap();
        let package = root.path().join("scalar.causalmed");
        project::save(stage.path(), &package).unwrap();
        let unpacked = project::unpack(&package, root.path()).unwrap();
        let restored = project::validate(unpacked.path()).unwrap();
        assert_eq!(restored.evaluations.len(), 3);
        fs::copy(
            unpacked.path().join("selected.json"),
            unpacked.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        let rw = Worker::new(unpacked.path());
        let (_, missing) = jobs(&restored, &rw).unwrap();
        assert_eq!(missing[0]["issue"], "file_changed");
        let moved = root.path().join("moved.h5ad");
        fs::rename(&source, &moved).unwrap();
        rw.reserve(120).unwrap();
        rw.run_relink(&h5ad::python_path(), h5ad::worker_script(), &moved, &sha);
        assert_eq!(rw.snapshot().status, "completed");
        let (ctx, input) = jobs(&restored, &rw).unwrap();
        rw.reserve(120).unwrap();
        rw.run_outcome(ctx, input);
        assert_eq!(rw.snapshot().status, "completed");
        assert_eq!(
            rw.snapshot().outcome_report.unwrap()["inputs"][0]["donors"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        let bs = ResearchStore::open(b.path().join("research.sqlite3")).unwrap();
        assert!(bs.snapshot().evaluations.is_empty());
        assert!(jobs(&bs.snapshot(), &Worker::new(b.path())).is_err());
        assert_eq!(project::digest(&moved).unwrap(), sha);
    }
}
