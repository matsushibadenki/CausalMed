//! Reversible local tombstones. Never delete/move source H5ADs or immutable QC artifacts.
use crate::{h5ad::Worker, project};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Tombstone {
    version: u32,
    id: String,
    scope: String,
    trashed_at: u128,
}
fn history_marker(folder: &Path, id: &str) -> PathBuf {
    folder.join(".trash").join("qc").join(format!("{id}.json"))
}
pub(crate) fn history_trashed(folder: &Path, id: &str) -> bool {
    history_marker(folder, id).exists()
}
/// Portable form carries no local owner or source path.
pub(crate) fn history_timestamp(
    folder: &Path,
    id: &str,
    scope: &str,
) -> Result<Option<u128>, String> {
    let path = history_marker(folder, id);
    if !path.exists() {
        return Ok(None);
    }
    if fs::metadata(&path).map_err(|_| "invalid_project")?.len() > 4096 {
        return Err("invalid_project".into());
    }
    let entry: Tombstone = serde_json::from_slice(&fs::read(path).map_err(|_| "invalid_project")?)
        .map_err(|_| "invalid_project")?;
    if entry.version != 1 || entry.id != id || entry.scope != scope {
        return Err("invalid_project".into());
    }
    Ok(Some(entry.trashed_at))
}
pub(crate) fn restore_portable_marker(
    folder: &Path,
    id: &str,
    trashed_at: u128,
) -> Result<(), String> {
    if !crate::h5ad::safe_id(id) {
        return Err("invalid_project".into());
    }
    let scope = folder
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid_project")?;
    let path = history_marker(folder, id);
    let parent = path.parent().ok_or("invalid_project")?;
    fs::create_dir_all(parent).map_err(|_| "invalid_project")?;
    let entry = Tombstone {
        version: 1,
        id: id.into(),
        scope: scope.into(),
        trashed_at,
    };
    fs::write(
        path,
        serde_json::to_vec(&entry).map_err(|_| "invalid_project")?,
    )
    .map_err(|_| "invalid_project".into())
}
pub(crate) fn project_trashed(folder: &Path) -> bool {
    folder.join(".project-trash.json").exists()
}
fn write_marker(path: &Path, id: &str, scope: &str) -> Result<(), String> {
    if path.exists() {
        return Err("trash_conflict".into());
    }
    let parent = path.parent().ok_or("storage_error")?;
    fs::create_dir_all(parent).map_err(|_| "storage_error")?;
    let entry = Tombstone {
        version: 1,
        id: id.into(),
        scope: scope.into(),
        trashed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    };
    let mut stage = tempfile::NamedTempFile::new_in(parent).map_err(|_| "storage_error")?;
    stage
        .write_all(&serde_json::to_vec(&entry).map_err(|_| "storage_error")?)
        .and_then(|_| stage.as_file().sync_all())
        .map_err(|_| "storage_error")?;
    stage
        .persist_noclobber(path)
        .map_err(|_| "trash_conflict")?;
    Ok(())
}
fn remove_marker(path: &Path, id: &str, scope: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|_| "trash_unavailable")?;
    if bytes.len() > 4096 {
        return Err("trash_unavailable".into());
    }
    let entry: Tombstone = serde_json::from_slice(&bytes).map_err(|_| "trash_unavailable")?;
    if entry.version != 1 || entry.id != id || entry.scope != scope {
        return Err("trash_unavailable".into());
    }
    fs::remove_file(path).map_err(|_| "storage_error".into())
}
pub(crate) fn history(worker: &Worker, scope: &str, id: &str, restore: bool) -> Result<(), String> {
    let snapshot = worker.snapshot();
    if snapshot.project_key != scope {
        return Err("project_changed".into());
    }
    if ["running", "selecting", "cancelling"].contains(&snapshot.status.as_str()) {
        return Err("worker_busy".into());
    }
    if !crate::h5ad::safe_id(id) {
        return Err("trash_unavailable".into());
    }
    let path = worker.folder().join("qc-runs").join(id);
    if fs::symlink_metadata(&path)
        .map_err(|_| "trash_unavailable")?
        .file_type()
        .is_symlink()
    {
        return Err("trash_unavailable".into());
    }
    let record = worker.archived_record(id)?; // Validate in this project, even when restoring.
    let marker = history_marker(worker.folder(), id);
    crate::search_index::invalidate_checked(worker.folder())?;
    if restore {
        if project::digest(&path.join("cells.npz")).map_err(|_| "trash_unavailable")?
            != record["report"]["selection"]["sha256"]
                .as_str()
                .ok_or("trash_unavailable")?
        {
            return Err("selection_changed".into());
        }
        remove_marker(&marker, id, scope)?;
    } else {
        // Clear the selected report before hiding its archive. A crash/error between these
        // operations can leave a live unselected archive, never a dangling selected result.
        if history_trashed(worker.folder(), id) {
            return Err("trash_conflict".into());
        }
        worker.clear_history_selection(id)?;
        write_marker(&marker, id, scope)?;
    }
    worker.history_changed();
    crate::search_index::invalidate(worker.folder());
    Ok(())
}
pub(crate) fn project(root: &Path, id: &str, restore: bool) -> Result<(), String> {
    let folder = project::resolve_any(root, id)?;
    if folder == project::active_folder(root)? {
        return Err("active_project".into());
    }
    let marker = folder.join(".project-trash.json");
    if restore {
        causalmed_core::ResearchStore::inspect(folder.join("research.sqlite3"))
            .map_err(|_| "trash_unavailable")?;
        remove_marker(&marker, id, id)
    } else {
        write_marker(&marker, id, id)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchResult {
    pub scope: String,
    pub succeeded: Vec<String>,
    pub failed: Vec<BatchFailure>,
}
#[derive(Serialize)]
pub(crate) struct BatchFailure {
    pub id: String,
    pub error: String,
}
struct Planned {
    id: String,
    marker: PathBuf,
    owner: String,
}
/// Caller holds the project write gate. Validate the entire selection before touching
/// any state; subsequent filesystem failures are reported per item, never hidden.
pub(crate) fn batch(
    root: &Path,
    worker: &Worker,
    scope: &str,
    kind: &str,
    ids: &[String],
    restore: bool,
) -> Result<BatchResult, String> {
    let snapshot = worker.snapshot();
    if snapshot.project_key != scope {
        return Err("project_changed".into());
    }
    if ["running", "selecting", "cancelling"].contains(&snapshot.status.as_str()) {
        return Err("worker_busy".into());
    }
    let mut seen = std::collections::HashSet::new();
    if ids.is_empty() || ids.len() > 10_000 || ids.iter().any(|id| !seen.insert(id)) {
        return Err("invalid_batch_selection".into());
    }
    let mut plan = Vec::with_capacity(ids.len());
    for id in ids {
        let (marker, owner) = match kind {
            "history" => {
                if !crate::h5ad::safe_id(id) {
                    return Err("trash_unavailable".into());
                }
                let path = worker.folder().join("qc-runs").join(id);
                if fs::symlink_metadata(&path)
                    .map_err(|_| "trash_unavailable")?
                    .file_type()
                    .is_symlink()
                {
                    return Err("trash_unavailable".into());
                }
                let record = worker.archived_record(id)?;
                if restore
                    && project::digest(&path.join("cells.npz")).map_err(|_| "trash_unavailable")?
                        != record["report"]["selection"]["sha256"]
                            .as_str()
                            .ok_or("trash_unavailable")?
                {
                    return Err("selection_changed".into());
                }
                (history_marker(worker.folder(), id), scope.to_string())
            }
            "project" => {
                let folder = project::resolve_any(root, id)?;
                if folder == project::active_folder(root)? {
                    return Err("active_project".into());
                }
                causalmed_core::ResearchStore::inspect(folder.join("research.sqlite3"))
                    .map_err(|_| "trash_unavailable")?;
                (folder.join(".project-trash.json"), id.clone())
            }
            _ => return Err("trash_unavailable".into()),
        };
        if marker.exists() != restore {
            return Err("trash_conflict".into());
        }
        if restore {
            if fs::metadata(&marker)
                .map_err(|_| "trash_unavailable")?
                .len()
                > 4096
            {
                return Err("trash_unavailable".into());
            }
            let entry: Tombstone =
                serde_json::from_slice(&fs::read(&marker).map_err(|_| "trash_unavailable")?)
                    .map_err(|_| "trash_unavailable")?;
            if entry.version != 1 || entry.id != *id || entry.scope != owner {
                return Err("trash_unavailable".into());
            }
        }
        plan.push(Planned {
            id: id.clone(),
            marker,
            owner,
        });
    }
    if kind == "history" {
        crate::search_index::invalidate_checked(worker.folder())?;
    }
    Ok(apply_batch(worker, scope, kind, plan, restore))
}
fn apply_batch(
    worker: &Worker,
    scope: &str,
    kind: &str,
    plan: Vec<Planned>,
    restore: bool,
) -> BatchResult {
    let mut result = BatchResult {
        scope: scope.into(),
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    for item in plan {
        let applied = (|| {
            if kind == "history" && !restore {
                worker.clear_history_selection(&item.id)?;
            }
            if restore {
                remove_marker(&item.marker, &item.id, &item.owner)
            } else {
                write_marker(&item.marker, &item.id, &item.owner)
            }
        })();
        match applied {
            Ok(()) => result.succeeded.push(item.id),
            Err(error) => result.failed.push(BatchFailure { id: item.id, error }),
        }
    }
    if kind == "history" {
        worker.history_changed();
        crate::search_index::invalidate(worker.folder());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        browse::{self, Query},
        h5ad, search_index,
    };
    use causalmed_core::ResearchStore;
    use serde_json::Value;
    fn history_page(w: &Worker, trash: bool) -> browse::Page<browse::HistoryRow> {
        browse::history(
            w,
            &w.snapshot().project_key,
            &Query {
                trash,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn selected_history_trash_restore_restart_conflicts_and_busy_guard() {
        let root = tempfile::tempdir().unwrap();
        let id = search_index::tests::fixture(root.path(), 1);
        let record = fs::read(root.path().join("qc-runs").join(&id).join("report.json")).unwrap();
        fs::write(root.path().join("h5ad-metadata.json"), &record).unwrap();
        let worker = Worker::new(root.path());
        let scope = worker.snapshot().project_key;
        assert_eq!(history_page(&worker, false).total, 1);
        assert!(history(&worker, "foreign", &id, false).is_err());
        worker.reserve(120).unwrap();
        assert_eq!(
            history(&worker, &scope, &id, false).unwrap_err(),
            "worker_busy"
        );
        worker.finish("cancelled", None);
        history(&worker, &scope, &id, false).unwrap();
        assert!(worker.snapshot().report.unwrap()["selection"].is_null());
        assert!(worker.snapshot().comparison.is_none());
        assert!(worker.select_history(&id).is_err());
        assert!(worker.export_source(&id).is_err());
        assert_eq!(history_page(&worker, false).total, 0);
        assert_eq!(history_page(&worker, true).total, 1);
        assert!(worker.history().is_empty());
        assert_eq!(
            fs::read(root.path().join("qc-runs").join(&id).join("report.json")).unwrap(),
            record
        );
        let restarted = Worker::new(root.path());
        assert!(restarted.snapshot().report.unwrap()["selection"].is_null());
        assert_eq!(
            history(&restarted, &scope, &id, false).unwrap_err(),
            "trash_conflict"
        );
        history(&restarted, &scope, &id, true).unwrap();
        assert!(restarted.snapshot().report.unwrap()["selection"].is_null());
        assert_eq!(history_page(&restarted, false).total, 1);
        assert_eq!(history_page(&restarted, true).total, 0);
        restarted.select_history(&id).unwrap();
        assert_eq!(restarted.snapshot().report.unwrap()["selection"]["id"], id);
        assert!(history(&restarted, &scope, "../../foreign", false).is_err());
        assert!(history(&restarted, &scope, &id, true).is_err());
        history(&restarted, &scope, &id, false).unwrap();
        fs::write(
            root.path().join("qc-runs").join(&id).join("cells.npz"),
            b"changed",
        )
        .unwrap();
        assert_eq!(
            history(&restarted, &scope, &id, true).unwrap_err(),
            "selection_changed"
        );
        assert_eq!(history_page(&restarted, true).total, 1);
    }
    #[test]
    fn failed_marker_write_leaves_live_archive_and_no_dangling_selection() {
        let root = tempfile::tempdir().unwrap();
        let id = search_index::tests::fixture(root.path(), 0);
        fs::write(
            root.path().join("h5ad-metadata.json"),
            fs::read(root.path().join("qc-runs").join(&id).join("report.json")).unwrap(),
        )
        .unwrap();
        let worker = Worker::new(root.path());
        let scope = worker.snapshot().project_key;
        fs::write(root.path().join(".trash"), b"blocked directory").unwrap();
        assert!(history(&worker, &scope, &id, false).is_err());
        assert!(!history_trashed(root.path(), &id));
        assert!(worker.snapshot().report.unwrap()["selection"].is_null());
        assert!(Worker::new(root.path()).snapshot().report.unwrap()["selection"].is_null());
        assert_eq!(history_page(&worker, false).total, 1);
        worker.select_history(&id).unwrap();
    }
    #[test]
    fn batch_preflight_selection_and_integrity_are_scoped() {
        let root = tempfile::tempdir().unwrap();
        let ids: Vec<_> = (0..2)
            .map(|i| search_index::tests::fixture(root.path(), i))
            .collect();
        let worker = Worker::new(root.path());
        let scope = worker.snapshot().project_key;
        worker.select_history(&ids[0]).unwrap();
        assert!(batch(root.path(), &worker, &scope, "history", &[], false).is_err());
        assert!(batch(
            root.path(),
            &worker,
            &scope,
            "history",
            &vec![ids[0].clone(); 10001],
            false
        )
        .is_err());
        assert!(batch(
            root.path(),
            &worker,
            &scope,
            "history",
            &[ids[0].clone(), ids[0].clone()],
            false
        )
        .is_err());
        assert!(batch(root.path(), &worker, "foreign", "history", &ids, false).is_err());
        assert!(batch(
            root.path(),
            &worker,
            &scope,
            "history",
            &[
                ids[0].clone(),
                "ffffffff-ffff-4fff-8fff-ffffffffffff".into()
            ],
            false
        )
        .is_err());
        assert_eq!(worker.snapshot().report.unwrap()["selection"]["id"], ids[0]);
        assert_eq!(history_page(&worker, false).total, 2);
        worker.reserve(120).unwrap();
        assert!(batch(root.path(), &worker, &scope, "history", &ids, false).is_err());
        worker.finish("cancelled", None);
        let result = batch(root.path(), &worker, &scope, "history", &ids, false).unwrap();
        assert_eq!(result.succeeded, ids);
        assert!(result.failed.is_empty());
        assert!(worker.snapshot().report.unwrap()["selection"].is_null());
        assert_eq!(history_page(&worker, true).total, 2);
        let path = root.path().join("qc-runs").join(&ids[1]).join("cells.npz");
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"changed").unwrap();
        assert!(batch(root.path(), &worker, &scope, "history", &ids, true).is_err());
        assert_eq!(history_page(&worker, true).total, 2);
        fs::write(path, original).unwrap();
        let restarted = Worker::new(root.path());
        let restored = batch(root.path(), &restarted, &scope, "history", &ids, true).unwrap();
        assert_eq!(restored.succeeded, ids);
        assert_eq!(history_page(&restarted, false).total, 2);
        assert!(restarted.snapshot().report.unwrap()["selection"].is_null());
        restarted.select_history(&ids[0]).unwrap();
    }
    #[test]
    fn batch_projects_preflight_active_then_move_and_restore_many() {
        let root = tempfile::tempdir().unwrap();
        ResearchStore::open(root.path().join("research.sqlite3")).unwrap();
        let worker = Worker::new(root.path());
        let scope = worker.snapshot().project_key;
        let folders: Vec<_> = (0..23)
            .map(|i| project::create(root.path(), &format!("Bulk {i}"), 0).unwrap())
            .collect();
        let ids: Vec<_> = folders
            .iter()
            .map(|f| f.path().file_name().unwrap().to_string_lossy().to_string())
            .collect();
        let mut invalid = ids.clone();
        invalid.push("local".into());
        assert!(batch(root.path(), &worker, &scope, "project", &invalid, false).is_err());
        assert_eq!(project::catalog(root.path()).unwrap().len(), 24);
        let moved = batch(root.path(), &worker, &scope, "project", &ids, false).unwrap();
        assert_eq!(moved.succeeded.len(), 23);
        assert!(moved.failed.is_empty());
        assert_eq!(
            crate::browse::projects(root.path(), &scope, &Query::default())
                .unwrap()
                .total,
            1
        );
        assert_eq!(
            crate::browse::projects(
                root.path(),
                &scope,
                &Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            23
        );
        let restored = batch(root.path(), &worker, &scope, "project", &ids, true).unwrap();
        assert_eq!(restored.succeeded.len(), 23);
        assert_eq!(project::catalog(root.path()).unwrap().len(), 24);
        assert_eq!(project::active_folder(root.path()).unwrap(), root.path());
    }
    #[test]
    fn interrupted_batch_rebuilds_index_from_partial_markers_on_restart() {
        let root = tempfile::tempdir().unwrap();
        let ids: Vec<_> = (0..2)
            .map(|i| search_index::tests::fixture(root.path(), i))
            .collect();
        let worker = Worker::new(root.path());
        let scope = worker.snapshot().project_key;
        worker.select_history(&ids[0]).unwrap();
        assert_eq!(history_page(&worker, false).total, 2);
        search_index::invalidate_checked(root.path()).unwrap();
        worker.clear_history_selection(&ids[0]).unwrap();
        write_marker(&history_marker(root.path(), &ids[0]), &ids[0], &scope).unwrap();
        // Model process exit before the batch publishes its final invalidation/receipt.
        drop(worker);
        let restarted = Worker::new(root.path());
        assert!(restarted.snapshot().report.unwrap()["selection"].is_null());
        assert_eq!(history_page(&restarted, false).total, 1);
        assert_eq!(history_page(&restarted, true).total, 1);
        let result = batch(
            root.path(),
            &restarted,
            &scope,
            "history",
            &[ids[0].clone()],
            true,
        )
        .unwrap();
        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(history_page(&restarted, false).total, 2);
    }
    #[test]
    fn batch_filesystem_failures_return_exact_partial_receipt() {
        let root = tempfile::tempdir().unwrap();
        let ids: Vec<_> = (0..2)
            .map(|i| search_index::tests::fixture(root.path(), i))
            .collect();
        let worker = Worker::new(root.path());
        let scope = worker.snapshot().project_key;
        worker.select_history(&ids[0]).unwrap();
        // Inject a write failure after planning, covering the per-item failure boundary.
        let blocked = root.path().join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        let plan = vec![
            Planned {
                id: ids[0].clone(),
                marker: blocked.join("marker.json"),
                owner: scope.clone(),
            },
            Planned {
                id: ids[1].clone(),
                marker: history_marker(root.path(), &ids[1]),
                owner: scope.clone(),
            },
        ];
        let result = apply_batch(&worker, &scope, "history", plan, false);
        assert_eq!(result.succeeded, vec![ids[1].clone()]);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].id, ids[0]);
        assert!(worker.snapshot().report.unwrap()["selection"].is_null());
        assert_eq!(history_page(&worker, false).total, 1);
        assert_eq!(history_page(&worker, true).total, 1);
        assert_eq!(Worker::new(root.path()).history().len(), 1);
    }
    #[test]
    fn project_trash_keeps_root_children_names_and_active_pointer_separate() {
        let root = tempfile::tempdir().unwrap();
        ResearchStore::open(root.path().join("research.sqlite3")).unwrap();
        let b = project::create(root.path(), "B", 0).unwrap();
        let bid = b.path().file_name().unwrap().to_str().unwrap().to_owned();
        project::activate(root.path(), b.path()).unwrap();
        assert_eq!(
            project(root.path(), &bid, false).unwrap_err(),
            "active_project"
        );
        let live = browse::projects(root.path(), &bid, &Query::default()).unwrap();
        assert_eq!(live.total, 2);
        project(root.path(), "local", false).unwrap();
        assert!(project::resolve(root.path(), "local").is_err());
        assert_eq!(
            browse::projects(root.path(), &bid, &Query::default())
                .unwrap()
                .total,
            1
        );
        let trashed = browse::projects(
            root.path(),
            &bid,
            &Query {
                trash: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(trashed.total, 1);
        assert_eq!(trashed.items[0].id, "local");
        assert_eq!(project::active_folder(root.path()).unwrap(), b.path());
        project(root.path(), "local", true).unwrap();
        assert_eq!(
            browse::projects(root.path(), &bid, &Query::default())
                .unwrap()
                .total,
            2
        );
        assert!(browse::projects(
            root.path(),
            &bid,
            &Query {
                trash: true,
                ..Default::default()
            }
        )
        .unwrap()
        .items
        .is_empty());
        project::activate(root.path(), root.path()).unwrap();
        project(root.path(), &bid, false).unwrap();
        assert!(project::prepare_switch(root.path(), &bid, 0).is_err());
        project(root.path(), &bid, true).unwrap();
        assert_eq!(
            ResearchStore::inspect(b.path().join("research.sqlite3"))
                .unwrap()
                .project_name,
            "B"
        );
    }
    #[test]
    #[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
    fn public_h5ad_trash_packages_restore_export_and_same_sha_isolation() {
        let root = tempfile::tempdir().unwrap();
        let a = project::create(root.path(), "Real A", 0).unwrap();
        let b = project::create(root.path(), "Real B", 0).unwrap();
        let aid = a.path().file_name().unwrap().to_str().unwrap().to_owned();
        let bid = b.path().file_name().unwrap().to_str().unwrap().to_owned();
        let source = PathBuf::from(std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap());
        let before = project::digest(&source).unwrap();
        let aw = Worker::new(a.path());
        let bw = Worker::new(b.path());
        for (w, options) in [
            (&aw, serde_json::json!({"donorColumn":"replicate"})),
            (&bw, serde_json::json!({"minGenes":300})),
        ] {
            w.reserve(120).unwrap();
            w.run_analysis(
                &h5ad::python_path(),
                h5ad::worker_script(),
                &source,
                &options,
                "",
            );
            assert_eq!(w.snapshot().status, "completed");
        }
        let ar = aw.snapshot().report.unwrap();
        let br = bw.snapshot().report.unwrap();
        let id = ar["selection"]["id"].as_str().unwrap();
        let other = br["selection"]["id"].as_str().unwrap();
        assert_eq!(ar["sha256"], br["sha256"]);
        aw.reserve(120).unwrap();
        aw.run_analysis(
            &h5ad::python_path(),
            h5ad::worker_script(),
            &source,
            &serde_json::json!({"minGenes":300,"donorColumn":"replicate"}),
            "",
        );
        let live = aw.snapshot().report.unwrap();
        let live_id = live["selection"]["id"].as_str().unwrap();
        aw.select_history(id).unwrap();
        project::activate(root.path(), a.path()).unwrap();
        assert_eq!(
            browse::projects(root.path(), &aid, &Query::default())
                .unwrap()
                .items
                .iter()
                .find(|r| r.id == aid)
                .unwrap()
                .history_count,
            2
        );
        let selected_ids = vec![id.to_string(), live_id.to_string()];
        let receipt = batch(root.path(), &aw, &aid, "history", &selected_ids, false).unwrap();
        assert_eq!(receipt.succeeded.len(), 2);
        assert!(batch(root.path(), &bw, &bid, "history", &selected_ids, true).is_err());
        assert_eq!(history_page(&bw, true).total, 0);
        let receipt = batch(root.path(), &aw, &aid, "history", &selected_ids, true).unwrap();
        assert_eq!(receipt.succeeded.len(), 2);
        assert!(aw.snapshot().report.unwrap()["selection"].is_null());
        aw.select_history(id).unwrap();
        history(&aw, &aid, id, false).unwrap();
        assert_eq!(history_page(&aw, false).total, 1);
        assert_eq!(history_page(&aw, true).total, 1);
        assert!(history(&bw, &bid, id, true).is_err());
        assert!(history(&aw, &bid, id, true).is_err());
        assert_eq!(history_page(&bw, false).items[0].id, other);
        assert_eq!(history_page(&bw, true).total, 0);
        let doc = ResearchStore::inspect(a.path().join("research.sqlite3")).unwrap();
        let stage = project::stage_save(a.path(), &doc, &aw).unwrap();
        assert!(stage.path().join("qc-runs").join(id).exists());
        let package = root.path().join("with-trash.causalmed");
        project::save(stage.path(), &package).unwrap();
        let unpacked = project::unpack(&package, root.path()).unwrap();
        fs::copy(
            unpacked.path().join("selected.json"),
            unpacked.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        let restored = Worker::new(unpacked.path());
        assert_eq!(history_page(&restored, false).total, 1);
        assert_eq!(history_page(&restored, false).items[0].id, live_id);
        assert_eq!(history_page(&restored, true).total, 1);
        assert!(restored.snapshot().report.unwrap()["selection"].is_null());
        let new_scope = restored.snapshot().project_key;
        assert!(history(&restored, &aid, id, true).is_err());
        let original_time = history_timestamp(a.path(), id, &aid).unwrap();
        assert_eq!(
            history_timestamp(unpacked.path(), id, &new_scope).unwrap(),
            original_time
        );
        assert!(!unpacked.path().join("history-search.sqlite3-wal").exists());
        assert!(!restored.snapshot().source_linked);
        history(&restored, &new_scope, id, true).unwrap();
        assert_eq!(history_page(&aw, true).total, 1);
        assert_eq!(history_page(&bw, true).total, 0);
        assert!(restored.snapshot().report.unwrap()["selection"].is_null());
        restored.reserve(120).unwrap();
        restored.run_relink(
            &h5ad::python_path(),
            h5ad::worker_script(),
            &source,
            &before,
        );
        restored.select_history(id).unwrap();
        assert_eq!(restored.snapshot().report.unwrap(), ar);
        let portable_output = root.path().join("portable-retained.h5ad");
        let selected = restored.export_source(id).unwrap();
        restored.reserve(120).unwrap();
        restored.run_export(
            &h5ad::python_path(),
            h5ad::worker_script(),
            selected,
            &portable_output,
            "export_h5ad",
        );
        assert_eq!(restored.snapshot().status, "completed");
        assert_eq!(restored.snapshot().last_export.unwrap()["retained"], 24562);
        let stage_again = project::stage_save(unpacked.path(), &doc, &restored).unwrap();
        let package_again = root.path().join("after-restore.causalmed");
        project::save(stage_again.path(), &package_again).unwrap();
        let again = project::unpack(&package_again, root.path()).unwrap();
        fs::copy(
            again.path().join("selected.json"),
            again.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        let again_worker = Worker::new(again.path());
        assert_eq!(history_page(&again_worker, false).total, 2);
        assert_eq!(history_page(&again_worker, true).total, 0);
        assert_eq!(
            again_worker.snapshot().report.unwrap()["selection"]["id"],
            id
        );

        project(root.path(), &bid, false).unwrap();
        assert_eq!(
            browse::projects(root.path(), &aid, &Query::default())
                .unwrap()
                .total,
            1
        );
        assert_eq!(
            browse::projects(
                root.path(),
                &aid,
                &Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .items[0]
                .id,
            bid
        );
        project(root.path(), &bid, true).unwrap();
        assert_eq!(Worker::new(b.path()).snapshot().report.unwrap(), br);
        let restarted = Worker::new(a.path());
        history(&restarted, &aid, id, true).unwrap();
        assert!(restarted.snapshot().report.unwrap()["selection"].is_null());
        restarted.select_history(id).unwrap();
        assert_eq!(restarted.snapshot().report.unwrap(), ar);
        let output = root.path().join("restored-retained.h5ad");
        let export = restarted.export_source(id).unwrap();
        restarted.reserve(120).unwrap();
        restarted.run_export(
            &h5ad::python_path(),
            h5ad::worker_script(),
            export,
            &output,
            "export_h5ad",
        );
        assert_eq!(restarted.snapshot().status, "completed");
        let result=std::process::Command::new(h5ad::python_path()).args(["-c","import anndata as a,numpy as n,sys; s=a.read_h5ad(sys.argv[1]); o=a.read_h5ad(sys.argv[2]); z=n.load(sys.argv[3],allow_pickle=False); m=z['retained']; assert o.shape==(24562,15706); assert (o.X!=s.X[m]).nnz==0; assert list(o.obs_names)==list(s.obs_names[m]); assert list(o.obs['replicate'])==list(s.obs['replicate'].iloc[n.flatnonzero(m)])"]).arg(&source).arg(&output).arg(a.path().join("qc-runs").join(id).join("cells.npz")).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(project::digest(&source).unwrap(), before);
        let rows = browse::projects(root.path(), &aid, &Query::default()).unwrap();
        assert_eq!(rows.total, 2);
        assert!(rows
            .items
            .iter()
            .all(|r| r.history_count == if r.id == aid { 2 } else { 1 }));
        let marker = fs::read(history_marker(a.path(), id)).ok();
        assert!(marker.is_none());
        let stored: Value = serde_json::from_slice(
            &fs::read(a.path().join("qc-runs").join(id).join("report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(stored["report"], ar);
    }
}
