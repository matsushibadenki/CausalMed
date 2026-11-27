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
