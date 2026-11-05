//! Disposable, project-local search projections. Immutable QC artifacts remain authoritative.
use crate::{
    browse::{HistoryRow, Page, Query},
    h5ad::Worker,
    project::{self, ProjectSummary},
};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

// Serialize cache reconciliation, including replacement of a damaged/schema-old cache.
// No worker completion takes this lock; it only writes an invalidation marker.
static CACHE_GATE: Mutex<()> = Mutex::new(());
const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO meta VALUES('version','2');
CREATE TABLE rows (id TEXT PRIMARY KEY, payload TEXT NOT NULL, search TEXT NOT NULL,
 name TEXT NOT NULL, created TEXT NOT NULL, retained INTEGER, excluded INTEGER,
 mapped INTEGER NOT NULL, status TEXT NOT NULL, histories INTEGER NOT NULL,
 active INTEGER NOT NULL, linked INTEGER NOT NULL, trashed INTEGER NOT NULL);
CREATE INDEX by_trash ON rows(trashed,id);
CREATE INDEX by_created ON rows(created,id);
CREATE INDEX by_name ON rows(name,id);
CREATE INDEX by_retained ON rows(retained,id);
CREATE INDEX by_mapping ON rows(mapped,id);
CREATE INDEX by_histories ON rows(histories,id);
CREATE VIRTUAL TABLE terms USING fts5(search, tokenize='trigram case_sensitive 1');
CREATE TRIGGER rows_delete AFTER DELETE ON rows BEGIN DELETE FROM terms WHERE rowid=old.rowid; END;
CREATE TRIGGER rows_insert AFTER INSERT ON rows BEGIN INSERT INTO terms(rowid,search) VALUES(new.rowid,new.search); END;
CREATE TRIGGER rows_update AFTER UPDATE ON rows BEGIN DELETE FROM terms WHERE rowid=old.rowid; INSERT INTO terms(rowid,search) VALUES(new.rowid,new.search); END;
";
fn error(_: impl std::fmt::Display) -> String {
    "search_index_error".into()
}
fn open(path: &Path) -> Result<Connection, String> {
    let existing = Connection::open(path).and_then(|c| {
        c.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: String =
            c.query_row("SELECT value FROM meta WHERE key='version'", [], |r| {
                r.get(0)
            })?;
        if version != "2" {
            return Err(rusqlite::Error::InvalidQuery);
        }
        // Access both tables so missing FTS structures trigger a rebuild as well.
        c.prepare("SELECT payload FROM rows LIMIT 0")?;
        c.prepare("SELECT search FROM terms LIMIT 0")?;
        Ok(c)
    });
    if let Ok(c) = existing {
        return Ok(c);
    }
    let stage = tempfile::NamedTempFile::new_in(path.parent().ok_or("search_index_error")?)
        .map_err(error)?;
    let c = Connection::open(stage.path()).map_err(error)?;
    c.execute_batch(SCHEMA).map_err(error)?;
    drop(c);
    stage.persist(path).map_err(error)?;
    Connection::open(path).map_err(error)
}
fn modified(path: &Path) -> String {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_default()
}
fn stamp(folder: &Path) -> String {
    format!(
        "{}:{}",
        modified(&folder.join("qc-runs")),
        fs::read_to_string(folder.join(".history-revision")).unwrap_or_default()
    )
}
/// O(1) notification after a committed analysis, including a commit racing cache reconciliation.
pub(crate) fn invalidate(folder: &Path) {
    let _ = invalidate_checked(folder);
}
/// Organization operations dirty the cache durably *before* their first mutation,
/// so a process interruption cannot leave a warm index hiding partial changes.
pub(crate) fn invalidate_checked(folder: &Path) -> Result<(), String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let mut f = tempfile::NamedTempFile::new_in(folder).map_err(error)?;
    use std::io::Write;
    f.write_all(value.as_bytes())
        .and_then(|_| f.as_file().sync_all())
        .map_err(error)?;
    f.persist(folder.join(".history-revision")).map_err(error)?;
    Ok(())
}

struct Entry<'a, T> {
    id: &'a str,
    payload: &'a T,
    search: String,
    name: String,
    created: String,
    retained: Option<u64>,
    excluded: Option<u64>,
    mapped: bool,
    status: &'a str,
    histories: usize,
    active: bool,
    linked: bool,
    trashed: bool,
}
fn upsert<T: Serialize>(c: &Connection, e: Entry<'_, T>) -> Result<(), String> {
    c.execute("INSERT INTO rows(id,payload,search,name,created,retained,excluded,mapped,status,histories,active,linked,trashed) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,search=excluded.search,name=excluded.name,created=excluded.created,retained=excluded.retained,excluded=excluded.excluded,mapped=excluded.mapped,status=excluded.status,histories=excluded.histories,active=excluded.active,linked=excluded.linked,trashed=excluded.trashed WHERE rows.payload<>excluded.payload",
        params![e.id,serde_json::to_string(e.payload).map_err(error)?,e.search.to_lowercase(),e.name.to_lowercase(),e.created,e.retained,e.excluded,e.mapped,e.status,e.histories,e.active,e.linked,e.trashed]).map_err(error)?;
    Ok(())
}
fn put_history(c: &Connection, r: &HistoryRow, trashed: bool) -> Result<(), String> {
    upsert(
        c,
        Entry {
            id: &r.id,
            payload: r,
            search: format!(
                "{} {} {} {} {} {}",
                r.name, r.id, r.source_sha256, r.created_at, r.status, r.options
            ),
            name: r.name.clone(),
            created: r.created_at.clone(),
            retained: Some(r.retained),
            excluded: Some(r.excluded),
            mapped: r.options["donorColumn"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            status: &r.status,
            histories: 0,
            active: false,
            linked: false,
            trashed,
        },
    )
}
fn ids(c: &Connection) -> Result<HashSet<String>, String> {
    c.prepare("SELECT id FROM rows")
        .map_err(error)?
        .query_map([], |r| r.get(0))
        .map_err(error)?
        .collect::<Result<_, _>>()
        .map_err(error)
}
fn reconcile(worker: &Worker) -> Result<Connection, String> {
    let mut c = open(&worker.folder().join("history-search.sqlite3"))?;
    let before = stamp(worker.folder());
    let previous: Option<String> = c
        .query_row("SELECT value FROM meta WHERE key='stamp'", [], |r| r.get(0))
        .ok();
    if previous.as_deref() == Some(&before) {
        return Ok(c);
    }
    let entries = match fs::read_dir(worker.folder().join("qc-runs")) {
        Ok(entries) => Some(entries),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(error(e)),
    };
    let tx = c.transaction().map_err(error)?;
    let known = ids(&tx)?;
    let mut seen = HashSet::new();
    if previous.is_none() {
        // Includes one-time migration and validation of legacy archives.
        let mut failure = None;
        worker.visit_archives(|v| {
            if let Some(row) = HistoryRow::from_report(v) {
                seen.insert(row.id.clone());
                if let Err(e) = put_history(
                    &tx,
                    &row,
                    crate::trash::history_trashed(worker.folder(), &row.id),
                ) {
                    failure = Some(e);
                }
            }
        });
        if let Some(e) = failure {
            return Err(e);
        }
    } else if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.map_err(error)?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !crate::h5ad::safe_id(&id) {
                continue;
            }
            if known.contains(&id) {
                let trashed = crate::trash::history_trashed(worker.folder(), &id);
                tx.execute(
                    "UPDATE rows SET trashed=?1 WHERE id=?2 AND trashed<>?1",
                    params![trashed, &id],
                )
                .map_err(error)?;
                seen.insert(id);
                continue;
            }
            if let Ok(record) = worker.archived_record(&id) {
                if let Some(row) = HistoryRow::from_report(record["report"].clone()) {
                    put_history(
                        &tx,
                        &row,
                        crate::trash::history_trashed(worker.folder(), &row.id),
                    )?;
                    seen.insert(id);
                }
            }
        }
    }
    for id in known.difference(&seen) {
        tx.execute("DELETE FROM rows WHERE id=?1", [id])
            .map_err(error)?;
    }
    // If a commit raced us, retain the old signature; the next query reconciles again.
    tx.execute("INSERT OR REPLACE INTO meta VALUES('stamp',?1)", [&before])
        .map_err(error)?;
    tx.commit().map_err(error)?;
    Ok(c)
}
pub(crate) fn history_count(worker: &Worker) -> Result<usize, String> {
    let _gate = CACHE_GATE.lock().map_err(error)?;
    reconcile(worker)?
        .query_row("SELECT count(*) FROM rows WHERE trashed=0", [], |r| {
            r.get(0)
        })
        .map_err(error)
}
pub(crate) fn history(
    worker: &Worker,
    selected: &str,
    scope: &str,
    q: &Query,
) -> Result<Page<HistoryRow>, String> {
    let _gate = CACHE_GATE.lock().map_err(error)?;
    let c = reconcile(worker)?;
    query(&c, selected, scope, q, false)
}
pub(crate) fn projects(
    root: &Path,
    scope: &str,
    q: &Query,
) -> Result<Page<ProjectSummary>, String> {
    // Catalog discovery reads bounded project metadata; history counts use each local index.
    let rows = project::catalog_all(root)?;
    let _gate = CACHE_GATE.lock().map_err(error)?;
    let mut c = open(&root.join("project-search.sqlite3"))?;
    let tx = c.transaction().map_err(error)?;
    let known = ids(&tx)?;
    let seen: HashSet<_> = rows.iter().map(|r| r.id.clone()).collect();
    for r in &rows {
        upsert(
            &tx,
            Entry {
                id: &r.id,
                payload: r,
                search: format!(
                    "{} {} {} {} {} {}",
                    r.name,
                    r.id,
                    r.donor_column.as_deref().unwrap_or(""),
                    r.source_name.as_deref().unwrap_or(""),
                    r.selected_id.as_deref().unwrap_or(""),
                    r.source_sha256.as_deref().unwrap_or("")
                ),
                name: r.name.clone(),
                created: r.created_at.map(|n| format!("{n:020}")).unwrap_or_default(),
                retained: r.retained,
                excluded: r.excluded,
                mapped: r.donor_column.as_ref().is_some_and(|s| !s.is_empty()),
                status: "",
                histories: r.history_count,
                active: r.active,
                linked: r.source_linked,
                trashed: r.trashed,
            },
        )?;
    }
    for id in known.difference(&seen) {
        tx.execute("DELETE FROM rows WHERE id=?1", [id])
            .map_err(error)?;
    }
    tx.commit().map_err(error)?;
    query(&c, "", scope, q, true)
}
fn query<T: DeserializeOwned>(
    c: &Connection,
    selected: &str,
    scope: &str,
    q: &Query,
    projects: bool,
) -> Result<Page<T>, String> {
    let mut clauses = vec!["trashed=?".to_string()];
    let mut values: Vec<SqlValue> = vec![i64::from(q.trash).into()];
    let tokens: Vec<_> = q.search.split_whitespace().map(str::to_lowercase).collect();
    let terms: Vec<_> = tokens
        .iter()
        .filter(|s| s.chars().count() >= 3)
        .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
        .collect();
    if !terms.is_empty() {
        clauses.push("rowid IN (SELECT rowid FROM terms WHERE terms MATCH ?)".into());
        values.push(terms.join(" AND ").into());
    }
    // Exact substring predicates preserve punctuation, wildcard literals and 1–2 character searches.
    for token in tokens {
        clauses.push("instr(search,?)>0".into());
        values.push(token.into());
    }
    let filter = match q.filter.as_str() {
        "selected" => {
            values.push(selected.to_string().into());
            "id=?"
        }
        "mapped" => "mapped=1",
        "unmapped" => "mapped=0",
        "partial" => "status='partial'",
        "complete" => "status='ready'",
        "with_exclusions" => "excluded>0",
        "empty_retained" => "retained=0",
        "active" => "active=1",
        "with_qc" => "histories>0",
        "without_qc" => "histories=0",
        "linked" => "linked=1",
        "missing" => "linked=0",
        _ => "1=1",
    };
    clauses.push(filter.into());
    let predicate = clauses.join(" AND ");
    let total: usize = c
        .query_row(
            &format!("SELECT count(*) FROM rows WHERE {predicate}"),
            params_from_iter(&values),
            |r| r.get(0),
        )
        .map_err(error)?;
    let unfiltered_total: usize = c
        .query_row(
            "SELECT count(*) FROM rows WHERE trashed=?1",
            [q.trash],
            |r| r.get(0),
        )
        .map_err(error)?;
    let pages = total.div_ceil(q.page_size).max(1);
    let page = q.page.min(pages);
    let order = match q.sort.as_str() {
        "oldest" => "created ASC",
        "name" => "name ASC",
        "retained_desc" => "retained DESC",
        "retained_asc" => "retained ASC",
        "history_desc" => "histories DESC",
        "active_first" if projects => "active DESC,created DESC",
        _ => "created DESC",
    };
    values.push((q.page_size as i64).into());
    values.push((((page - 1) * q.page_size) as i64).into());
    let mut stmt = c
        .prepare(&format!(
            "SELECT payload FROM rows WHERE {predicate} ORDER BY {order},id ASC LIMIT ? OFFSET ?"
        ))
        .map_err(error)?;
    let payloads = stmt
        .query_map(params_from_iter(&values), |r| r.get::<_, String>(0))
        .map_err(error)?;
    let items = payloads
        .map(|r| serde_json::from_str(&r.map_err(error)?).map_err(error))
        .collect::<Result<Vec<T>, String>>()?;
    Ok(Page {
        scope: scope.into(),
        items,
        total,
        unfiltered_total,
        page,
        page_size: q.page_size,
        pages,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    pub(crate) fn fixture(folder: &Path, i: usize) -> String {
        let id = format!("00000000-0000-4000-8000-{i:012x}");
        let dir = folder.join("qc-runs").join(&id);
        fs::create_dir_all(&dir).unwrap();
        let cell_bytes = b"Synthetic load fixture; not biomedical cell decisions";
        let selection_sha = causalmed_core::checksum(cell_bytes);
        fs::write(dir.join("cells.npz"), cell_bytes).unwrap();
        let qc = json!({"status":"partial","options":{"donorColumn":if i.is_multiple_of(2){"replicate"}else{""},"minGenes":200},"retained":i,"excluded":10001-i});
        let manifest=json!({"analysisId":id,"source":{"cells":10001,"genes":5,"sha256":"synthetic"},"qc":qc,"selectionSha256":selection_sha}).to_string();
        let report = json!({"report":{"name":format!("Synthetic load fixture {i:05} 東京 %_ \"quote\""),"cells":10001,"genes":5,"sha256":"synthetic","qc":qc,"selection":{"id":id,"createdAt":"2026-11-05T00:00:00Z","retained":i,"excluded":10001-i,"sha256":selection_sha,"manifestSha256":causalmed_core::checksum(manifest.as_bytes())}}});
        fs::write(dir.join("manifest.json"), manifest).unwrap();
        fs::write(dir.join("report.json"), report.to_string()).unwrap();
        id
    }
    fn browse(worker: &Worker, q: Query) -> Page<HistoryRow> {
        crate::browse::history(worker, &worker.snapshot().project_key, &q).unwrap()
    }
    #[test]
    fn ten_thousand_disk_histories_incremental_restart_and_isolation() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for i in 0..10000 {
            fixture(a.path(), i);
        }
        fixture(b.path(), 10000);
        let aw = Worker::new(a.path());
        let bw = Worker::new(b.path());
        crate::h5ad::HISTORY_READS.with(|n| n.set(0));
        let start = std::time::Instant::now();
        let cold = browse(
            &aw,
            Query {
                page_size: 50,
                ..Default::default()
            },
        );
        let cold_ms = start.elapsed().as_millis();
        crate::h5ad::HISTORY_READS.with(|n| {
            assert_eq!(n.get(), 10000);
            n.set(0);
        });
        assert_eq!(cold.total, 10000);
        assert_eq!(cold.pages, 200);
        assert_eq!(cold.items.len(), 50);
        let cache = a.path().join("history-search.sqlite3");
        let before = modified(&cache);
        let start = std::time::Instant::now();
        let warm = browse(
            &aw,
            Query {
                search: "REPLICATE 200".into(),
                filter: "mapped".into(),
                sort: "retained_desc".into(),
                page: 100,
                page_size: 50,
                trash: false,
            },
        );
        let warm_ms = start.elapsed().as_millis();
        crate::h5ad::HISTORY_READS
            .with(|n| assert_eq!(n.get(), 0, "warm queries must not read archived reports"));
        assert_eq!(warm.total, 5000);
        assert_eq!(warm.items[0].retained, 98);
        assert_eq!(warm.items[49].retained, 0);
        assert_eq!(
            modified(&cache),
            before,
            "warm queries must not rewrite the index"
        );
        // Existing substring semantics: Japanese short words, literal wildcard characters,
        // punctuation, quotes, and an FTS-looking token remain data, never SQL/FTS syntax.
        for term in ["東京", "%_", "\"quote\"", "東京 %_ quote"] {
            assert_eq!(
                browse(
                    &aw,
                    Query {
                        search: term.into(),
                        ..Default::default()
                    }
                )
                .total,
                10000
            );
        }
        assert_eq!(
            browse(
                &aw,
                Query {
                    search: "NOT OR *".into(),
                    ..Default::default()
                }
            )
            .total,
            0
        );
        crate::h5ad::HISTORY_READS.with(|n| n.set(0));
        let added = fixture(a.path(), 10000);
        invalidate(a.path());
        assert_eq!(browse(&aw, Query::default()).total, 10001);
        crate::h5ad::HISTORY_READS.with(|n| {
            assert_eq!(
                n.get(),
                1,
                "incremental reconciliation reads only the added report"
            )
        });
        assert_eq!(
            browse(
                &aw,
                Query {
                    search: added.clone(),
                    ..Default::default()
                }
            )
            .total,
            1
        );
        fs::remove_dir_all(a.path().join("qc-runs").join(&added)).unwrap();
        invalidate(a.path());
        assert_eq!(browse(&aw, Query::default()).total, 10000);
        let restarted = Worker::new(a.path());
        assert_eq!(
            browse(
                &restarted,
                Query {
                    search: added.clone(),
                    ..Default::default()
                }
            )
            .total,
            0
        );
        assert_eq!(
            browse(
                &bw,
                Query {
                    search: added,
                    ..Default::default()
                }
            )
            .total,
            1
        );
        assert_eq!(
            browse(
                &bw,
                Query {
                    search: "replicate".into(),
                    ..Default::default()
                }
            )
            .total,
            1
        );
        assert_eq!(browse(&bw, Query::default()).unfiltered_total, 1);
        // Reversible trash over a large indexed archive, without reopening existing
        // reports during reconciliation. Fixtures remain explicitly synthetic.
        let affected: Vec<_> = (0..1000)
            .map(|i| format!("00000000-0000-4000-8000-{i:012x}"))
            .collect();
        let scope = aw.snapshot().project_key;
        let moved =
            crate::trash::batch(a.path(), &aw, &scope, "history", &affected, false).unwrap();
        assert_eq!(moved.succeeded.len(), 1000);
        assert!(moved.failed.is_empty());
        crate::h5ad::HISTORY_READS.with(|n| n.set(0));
        assert_eq!(browse(&aw, Query::default()).total, 9000);
        assert_eq!(
            browse(
                &aw,
                Query {
                    trash: true,
                    page_size: 50,
                    page: 2,
                    ..Default::default()
                }
            )
            .items
            .len(),
            50
        );
        crate::h5ad::HISTORY_READS.with(|n| assert_eq!(n.get(), 0));
        let restarted = Worker::new(a.path());
        assert_eq!(
            browse(
                &restarted,
                Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .total,
            1000
        );
        assert_eq!(
            browse(
                &bw,
                Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .total,
            0
        );
        let restored =
            crate::trash::batch(a.path(), &restarted, &scope, "history", &affected, true).unwrap();
        assert_eq!(restored.succeeded.len(), 1000);
        assert!(restored.failed.is_empty());
        assert_eq!(browse(&restarted, Query::default()).total, 10000);
        assert_eq!(
            browse(
                &restarted,
                Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .total,
            0
        );
        let c = Connection::open(cache).unwrap();
        let plan: String = c
            .query_row(
                "EXPLAIN QUERY PLAN SELECT rowid FROM terms WHERE terms MATCH 'replicate'",
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(plan.contains("VIRTUAL TABLE INDEX"), "{plan}");
        eprintln!(
            "SEARCH_INDEX_LOAD histories=10000 cold_ms={cold_ms} warm_filtered_page_ms={warm_ms}"
        );
    }
    #[test]
    fn damaged_or_old_cache_rebuilds_and_selected_filter_is_live() {
        let root = tempfile::tempdir().unwrap();
        let id = fixture(root.path(), 0);
        let w = Worker::new(root.path());
        assert_eq!(browse(&w, Query::default()).total, 1);
        let path = root.path().join("history-search.sqlite3");
        fs::write(&path, b"damaged cache").unwrap();
        assert_eq!(browse(&w, Query::default()).items[0].id, id);
        let c = Connection::open(&path).unwrap();
        c.execute("UPDATE meta SET value='0' WHERE key='version'", [])
            .unwrap();
        drop(c);
        assert_eq!(browse(&w, Query::default()).total, 1);
        let _gate = CACHE_GATE.lock().unwrap();
        let c = reconcile(&w).unwrap();
        let q = Query {
            filter: "selected".into(),
            ..Default::default()
        };
        assert_eq!(
            query::<HistoryRow>(&c, &id, "A", &q, false).unwrap().total,
            1
        );
        assert_eq!(
            query::<HistoryRow>(&c, "other", "A", &q, false)
                .unwrap()
                .total,
            0
        );
    }
}
