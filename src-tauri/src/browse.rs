//! Scoped metadata queries. Matrices and per-cell decisions never enter list responses.
use crate::{h5ad::Worker, project::ProjectSummary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Query {
    pub search: String,
    pub filter: String,
    pub sort: String,
    pub page: usize,
    pub page_size: usize,
    pub trash: bool,
    pub source_sha256: String,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: "all".into(),
            sort: "newest".into(),
            page: 1,
            page_size: 10,
            trash: false,
            source_sha256: String::new(),
        }
    }
}
impl Query {
    fn validate(&self, filters: &[&str], sorts: &[&str]) -> Result<(), String> {
        if (!self.source_sha256.is_empty()
            && (self.source_sha256.len() != 64
                || !self
                    .source_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))))
            || self.search.len() > 512
            || self.search.contains('\0')
            || self.page == 0
            || !(1..=50).contains(&self.page_size)
            || !filters.contains(&self.filter.as_str())
            || !sorts.contains(&self.sort.as_str())
        {
            return Err("invalid_list_query".into());
        }
        Ok(())
    }
    #[cfg(test)]
    fn matches(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.search
            .split_whitespace()
            .all(|token| lower.contains(&token.to_lowercase()))
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub scope: String,
    pub items: Vec<T>,
    pub total: usize,
    pub unfiltered_total: usize,
    pub page: usize,
    pub page_size: usize,
    pub pages: usize,
}
#[cfg(test)]
fn paginate<T>(scope: String, items: Vec<T>, unfiltered_total: usize, q: &Query) -> Page<T> {
    let total = items.len();
    let pages = total.div_ceil(q.page_size).max(1);
    let page = q.page.min(pages);
    let items = items
        .into_iter()
        .skip((page - 1) * q.page_size)
        .take(q.page_size)
        .collect();
    Page {
        scope,
        items,
        total,
        unfiltered_total,
        page,
        page_size: q.page_size,
        pages,
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub source_sha256: String,
    pub cells: u64,
    pub retained: u64,
    pub excluded: u64,
    pub status: String,
    pub options: Value,
}
impl HistoryRow {
    pub(crate) fn from_report(r: Value) -> Option<Self> {
        Some(Self {
            id: r["selection"]["id"].as_str()?.into(),
            name: r["name"].as_str()?.into(),
            created_at: r["selection"]["createdAt"].as_str()?.into(),
            source_sha256: r["sha256"].as_str()?.into(),
            cells: r["cells"].as_u64()?,
            retained: r["selection"]["retained"].as_u64()?,
            excluded: r["selection"]["excluded"].as_u64()?,
            status: r["qc"]["status"].as_str()?.into(),
            options: r["qc"]["options"].clone(),
        })
    }
}
pub fn history(worker: &Worker, scope: &str, q: &Query) -> Result<Page<HistoryRow>, String> {
    q.validate(
        &[
            "all",
            "selected",
            "mapped",
            "unmapped",
            "partial",
            "complete",
            "with_exclusions",
            "empty_retained",
        ],
        &["newest", "oldest", "name", "retained_desc", "retained_asc"],
    )?;
    let snapshot = worker.snapshot();
    if snapshot.project_key != scope {
        return Err("project_changed".into());
    }
    let selected = snapshot
        .report
        .as_ref()
        .and_then(|r| r["selection"]["id"].as_str())
        .unwrap_or("");
    crate::search_index::history(worker, selected, scope, q)
}
#[cfg(test)]
fn history_rows(
    mut rows: Vec<HistoryRow>,
    selected: &str,
    scope: &str,
    q: &Query,
) -> Result<Page<HistoryRow>, String> {
    let unfiltered = rows.len();
    rows.retain(|r| {
        let mapped = r.options["donorColumn"]
            .as_str()
            .is_some_and(|s| !s.is_empty());
        let filter = match q.filter.as_str() {
            "selected" => r.id == selected,
            "mapped" => mapped,
            "unmapped" => !mapped,
            "partial" => r.status == "partial",
            "complete" => r.status == "ready",
            "with_exclusions" => r.excluded > 0,
            "empty_retained" => r.retained == 0,
            _ => true,
        };
        filter
            && q.matches(&format!(
                "{} {} {} {} {} {}",
                r.name, r.id, r.source_sha256, r.created_at, r.status, r.options
            ))
    });
    rows.sort_by(|a, b| {
        let order = match q.sort.as_str() {
            "oldest" => a.created_at.cmp(&b.created_at),
            "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            "retained_desc" => b.retained.cmp(&a.retained),
            "retained_asc" => a.retained.cmp(&b.retained),
            _ => b.created_at.cmp(&a.created_at),
        };
        order.then_with(|| a.id.cmp(&b.id))
    });
    Ok(paginate(scope.into(), rows, unfiltered, q))
}
pub fn projects(root: &Path, scope: &str, q: &Query) -> Result<Page<ProjectSummary>, String> {
    q.validate(
        &[
            "all",
            "active",
            "with_qc",
            "without_qc",
            "linked",
            "missing",
            "mapped",
            "unmapped",
        ],
        &[
            "active_first",
            "newest",
            "oldest",
            "name",
            "history_desc",
            "retained_desc",
            "retained_asc",
        ],
    )?;
    crate::search_index::projects(root, scope, q)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project;
    #[test]
    fn bounded_queries_and_stable_large_pages() {
        let rows=(0..503).map(|i|HistoryRow{id:format!("{i:04}"),name:"same".into(),created_at:"same".into(),source_sha256:"abc".into(),cells:1000,retained:i,excluded:1000-i,status:"partial".into(),options:serde_json::json!({"donorColumn":if i%2==0{"replicate"}else{""},"minGenes":200})}).collect::<Vec<_>>();
        let q = Query {
            page: 2,
            page_size: 50,
            sort: "name".into(),
            ..Query::default()
        };
        let p = history_rows(rows.clone(), "0002", "A", &q).unwrap();
        assert_eq!(p.total, 503);
        assert_eq!(p.items.len(), 50);
        assert_eq!(p.items[0].id, "0050");
        assert_eq!(p.pages, 11);
        let q = Query {
            search: "REPLICATE 200".into(),
            filter: "mapped".into(),
            sort: "retained_desc".into(),
            page: 999,
            page_size: 50,
            trash: false,
            source_sha256: String::new(),
        };
        let p = history_rows(rows, "0002", "A", &q).unwrap();
        assert_eq!(p.total, 252);
        assert_eq!(p.page, 6);
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[1].id, "0000");
        for q in [
            Query {
                page: 0,
                ..Query::default()
            },
            Query {
                page_size: 51,
                ..Query::default()
            },
            Query {
                search: "x".repeat(513),
                ..Query::default()
            },
            Query {
                filter: "bogus".into(),
                ..Query::default()
            },
        ] {
            assert!(q.validate(&["all"], &["newest"]).is_err());
        }
    }
    #[test]
    fn project_pages_filter_and_clamp_without_changing_active_project() {
        let root = tempfile::tempdir().unwrap();
        let mut folders = Vec::new();
        for i in 0..23 {
            folders.push(
                project::create(
                    root.path(),
                    &if i == 22 {
                        "Study target-22".into()
                    } else {
                        format!("Study {i:02}")
                    },
                    0,
                )
                .unwrap(),
            );
        }
        project::activate(root.path(), folders[0].path()).unwrap();
        let before = project::active_folder(root.path()).unwrap();
        let q = Query {
            sort: "name".into(),
            page_size: 10,
            page: 3,
            ..Default::default()
        };
        let p = projects(root.path(), "scope", &q).unwrap();
        assert_eq!(p.total, 23);
        assert_eq!(p.items.len(), 3);
        assert_eq!(p.items[0].name, "Study 20");
        let p = projects(
            root.path(),
            "scope",
            &Query {
                filter: "active".into(),
                page: 99,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 1);
        assert_eq!(p.page, 1);
        assert_eq!(p.items[0].name, "Study 00");
        let p = projects(
            root.path(),
            "scope",
            &Query {
                search: "study target-22".into(),
                filter: "without_qc".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 1);
        assert_eq!(p.unfiltered_total, 23);
        assert_eq!(project::active_folder(root.path()).unwrap(), before);
    }
    #[test]
    fn empty_query_and_selected_filter_have_consistent_counts() {
        let q = Query {
            search: "absent".into(),
            ..Query::default()
        };
        let p: Page<HistoryRow> = history_rows(vec![], "", "A", &q).unwrap();
        assert_eq!(p.page, 1);
        assert_eq!(p.pages, 1);
        assert_eq!(p.total, 0);
        assert!(p.items.is_empty());
        let root = tempfile::tempdir().unwrap();
        let worker = Worker::new(root.path());
        assert!(history(&worker, "foreign", &Query::default()).is_err());
    }
}
