use std::fs;

use super::*;
use crate::model::UrlPatternKind;
use crate::storage::corrupt_path;

fn database() -> HistoryDb {
    HistoryDb::in_memory().unwrap()
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("haku-history-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.join("history.sqlite")
}

fn visit(url: &str, title: &str, at: i64) -> NewVisit<'static> {
    visit_by(url, title, at, Transition::Link)
}

fn visit_by(url: &str, title: &str, at: i64, transition: Transition) -> NewVisit<'static> {
    NewVisit {
        url: Box::leak(url.to_string().into_boxed_str()),
        title: Box::leak(title.to_string().into_boxed_str()),
        visited_at: at,
        transition,
    }
}

fn everything() -> HistoryFilter {
    HistoryFilter::default()
}

fn urls(database: &HistoryDb, filter: &HistoryFilter) -> Vec<String> {
    database
        .query(filter, None, QUERY_LIMIT_MAX)
        .unwrap()
        .visits
        .into_iter()
        .map(|visit| visit.url)
        .collect()
}

fn all(database: &HistoryDb) -> Vec<HistoryVisit> {
    database.query(&everything(), None, QUERY_LIMIT_MAX).unwrap().visits
}

/// A page's stored counts: visits, typed visits, last visit.
fn counts(database: &HistoryDb, url: &str) -> Option<(i64, i64, i64)> {
    database
        .connection
        .query_row(
            "SELECT visit_count, typed_count, last_visited_at FROM pages WHERE url = ?1",
            [url],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .unwrap()
}

fn text(value: &str) -> HistoryFilter {
    HistoryFilter {
        text: Some(value.into()),
        ..everything()
    }
}

fn including(kind: UrlPatternKind, value: &str) -> HistoryFilter {
    HistoryFilter {
        include: vec![UrlPattern::new(kind, value)],
        ..everything()
    }
}

#[test]
fn a_damaged_file_is_set_aside_and_a_fresh_database_opens_in_its_place() {
    let path = temp_path("damaged");
    fs::write(&path, "this is not a database, just bytes\n").unwrap();

    let database = HistoryDb::open_or_set_aside(&path).unwrap();

    assert!(all(&database).is_empty());
}

#[test]
fn the_damaged_file_is_preserved_at_the_corrupt_path() {
    let path = temp_path("preserved");
    fs::write(&path, "this is not a database, just bytes\n").unwrap();

    HistoryDb::open_or_set_aside(&path).unwrap();

    assert_eq!(
        fs::read_to_string(corrupt_path(&path)).unwrap(),
        "this is not a database, just bytes\n"
    );
}

#[test]
fn a_file_that_cannot_be_opened_leaves_history_in_memory() {
    let path = temp_path("unopenable");
    fs::create_dir_all(&path).unwrap();

    let (database, failure) = HistoryDb::open_or_in_memory(&path);
    database.record(visit("https://a.test", "A", 1)).unwrap();

    assert_eq!(all(&database).len(), 1);
    assert!(path.is_dir());
    assert!(matches!(failure, Some(HakuError::Storage(_))));
}

#[test]
fn a_file_that_opens_reports_no_failure() {
    let path = temp_path("opens");

    let (_, failure) = HistoryDb::open_or_in_memory(&path);

    assert_eq!(failure, None);
}

#[test]
fn a_file_another_connection_holds_locked_leaves_history_in_memory_and_is_cleared_once_free() {
    let path = temp_path("locked");
    HistoryDb::open(&path)
        .unwrap()
        .record(visit("https://a.test", "A", 1))
        .unwrap();
    let holder = Connection::open(&path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();

    let (database, _) = HistoryDb::open_or_in_memory(&path);
    database.record(visit("https://b.test", "B", 2)).unwrap();
    assert_eq!(all(&database).len(), 1);
    assert!(database.delete(&everything()).is_err());

    holder.execute_batch("ROLLBACK").unwrap();
    database.delete(&everything()).unwrap();
    assert!(all(&HistoryDb::open(&path).unwrap()).is_empty());
}

#[test]
fn a_filtered_delete_while_in_memory_reaches_the_file_too() {
    let path = temp_path("filtered-unopened");
    let file = HistoryDb::open(&path).unwrap();
    file.record(visit("https://a.test/", "A", 1)).unwrap();
    file.record(visit("https://b.test/", "B", 2)).unwrap();
    drop(file);
    let holder = Connection::open(&path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let (database, _) = HistoryDb::open_or_in_memory(&path);
    holder.execute_batch("ROLLBACK").unwrap();

    database.delete(&including(UrlPatternKind::Site, "a.test")).unwrap();

    assert_eq!(
        urls(&HistoryDb::open(&path).unwrap(), &everything()),
        ["https://b.test/"]
    );
}

#[test]
fn a_healthy_database_is_opened_as_it_is_and_not_moved() {
    let path = temp_path("healthy");
    HistoryDb::open(&path)
        .unwrap()
        .record(visit("https://a.test", "A", 1))
        .unwrap();

    let database = HistoryDb::open_or_set_aside(&path).unwrap();

    assert_eq!(all(&database).len(), 1);
    assert!(!corrupt_path(&path).exists());
}

#[test]
fn a_fresh_database_is_migrated_to_the_latest_revision() {
    let database = database();
    let version: i64 = database
        .connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();

    assert_eq!(version as usize, MIGRATIONS.len());
}

#[test]
fn migrating_an_already_current_database_changes_nothing() {
    let mut database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();

    sqlite::migrate(&mut database.connection, MIGRATIONS).unwrap();

    assert_eq!(all(&database).len(), 1);
}

/// A database at the second revision, with visits written as it wrote them.
fn second_revision(visits: &[(&str, &str, i64)]) -> HistoryDb {
    let mut connection = Connection::open_in_memory().unwrap();
    sqlite::migrate(&mut connection, &MIGRATIONS[..2]).unwrap();
    for (url, title, at) in visits {
        connection
            .execute(
                "INSERT INTO visits (url, title, visited_at) VALUES (?1, ?2, ?3)",
                params![url, title, at],
            )
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO downloads (id, url, path, received, total, state, failure, started_at)
             VALUES (1, 'https://a.test/f.zip', 'C:\\f.zip', 1, 1, 'completed', NULL, 5)",
            [],
        )
        .unwrap();
    sqlite::migrate(&mut connection, MIGRATIONS).unwrap();
    HistoryDb::wrap(connection)
}

#[test]
fn migrating_from_the_second_revision_keeps_every_visit_as_other() {
    let database = second_revision(&[
        ("https://a.test/", "A old", 1),
        ("https://b.test/", "B", 2),
        ("https://a.test/", "A new", 3),
    ]);

    let visits = all(&database);

    assert_eq!(
        visits
            .iter()
            .map(|v| (v.url.as_str(), v.visited_at))
            .collect::<Vec<_>>(),
        [("https://a.test/", 3), ("https://b.test/", 2), ("https://a.test/", 1)]
    );
    assert!(visits.iter().all(|v| v.transition == Transition::Other));
}

#[test]
fn migrating_gives_each_page_its_host_latest_title_and_counts() {
    let database = second_revision(&[
        ("https://Mail.A.test:8080/x", "Old", 1),
        ("https://Mail.A.test:8080/x", "New", 3),
        ("file:///C:/notes.txt", "Notes", 2),
    ]);

    let visits = all(&database);

    assert_eq!(visits[0].host, "mail.a.test");
    assert_eq!(visits[0].title, "New");
    assert_eq!(visits[1].host, "");
    assert_eq!(counts(&database, "https://Mail.A.test:8080/x"), Some((2, 0, 3)));
}

#[test]
fn migrating_keeps_downloads_and_the_text_index_finds_migrated_pages() {
    let database = second_revision(&[("https://a.test/", "Quarterly report", 1)]);

    assert_eq!(database.downloads().unwrap().len(), 1);
    assert_eq!(urls(&database, &text("quarter")), ["https://a.test/"]);
}

#[test]
fn a_recorded_visit_is_read_back_with_its_host_and_transition() {
    let database = database();
    database
        .record(visit_by(
            "https://www.a.test/x",
            "A",
            1_700_000_000_000,
            Transition::Typed,
        ))
        .unwrap();

    let visits = all(&database);

    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].url, "https://www.a.test/x");
    assert_eq!(visits[0].host, "www.a.test");
    assert_eq!(visits[0].visited_at, 1_700_000_000_000);
    assert_eq!(visits[0].transition, Transition::Typed);
}

#[test]
fn reloading_the_same_page_updates_the_visit_rather_than_duplicating_it() {
    let database = database();
    database.record(visit("https://a.test", "Loading", 1)).unwrap();
    database.record(visit("https://a.test", "Ready", 2)).unwrap();

    let visits = all(&database);

    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].title, "Ready");
    assert_eq!(visits[0].visited_at, 2);
    assert_eq!(counts(&database, "https://a.test"), Some((1, 0, 2)));
}

#[test]
fn returning_to_a_page_after_visiting_another_records_a_new_visit() {
    let database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();
    database.record(visit("https://b.test", "B", 2)).unwrap();
    database.record(visit("https://a.test", "A", 3)).unwrap();

    assert_eq!(all(&database).len(), 3);
    assert_eq!(counts(&database, "https://a.test"), Some((2, 0, 3)));
}

#[test]
fn typed_visits_are_counted_apart() {
    let database = database();
    database
        .record(visit_by("https://a.test", "A", 1, Transition::Typed))
        .unwrap();
    database.record(visit("https://b.test", "B", 2)).unwrap();
    database.record(visit("https://a.test", "A", 3)).unwrap();
    database.record(visit("https://b.test", "B", 4)).unwrap();
    database
        .record(visit_by("https://a.test", "A", 5, Transition::Search))
        .unwrap();

    assert_eq!(counts(&database, "https://a.test"), Some((3, 1, 5)));
}

#[test]
fn every_visit_to_a_page_shows_its_latest_title() {
    let database = database();
    database.record(visit("https://a.test", "Old", 1)).unwrap();
    database.record(visit("https://b.test", "B", 2)).unwrap();
    database.record(visit("https://a.test", "New", 3)).unwrap();

    let titles: Vec<String> = all(&database)
        .into_iter()
        .filter(|v| v.url == "https://a.test")
        .map(|v| v.title)
        .collect();

    assert_eq!(titles, ["New", "New"]);
}

#[test]
fn retitling_changes_the_page_title_without_adding_a_visit() {
    let database = database();
    database.record(visit("https://a.test", "Old", 1)).unwrap();

    database.retitle("https://a.test", "New").unwrap();

    let visits = all(&database);
    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].title, "New");
    assert_eq!(urls(&database, &text("New")), ["https://a.test"]);
    assert!(urls(&database, &text("Old")).is_empty());
}

#[test]
fn retitling_a_url_never_visited_records_nothing() {
    let database = database();

    database.retitle("https://a.test", "A").unwrap();

    assert!(all(&database).is_empty());
}

#[test]
fn queries_are_newest_first_and_respect_their_limit() {
    let database = database();
    for index in 0..5 {
        database
            .record(visit(&format!("https://{index}.test"), "T", index))
            .unwrap();
    }

    let page = database.query(&everything(), None, 2).unwrap();

    assert_eq!(
        page.visits.iter().map(|v| v.url.as_str()).collect::<Vec<_>>(),
        ["https://4.test", "https://3.test"]
    );
    assert_eq!(
        page.next,
        Some(HistoryCursor {
            visited_at: 3,
            id: page.visits[1].id
        })
    );
}

#[test]
fn paging_by_cursor_reads_every_visit_once_even_at_equal_times() {
    let database = database();
    for index in 0..7 {
        database
            .record(visit(&format!("https://{index}.test"), "T", index / 3))
            .unwrap();
    }

    let mut seen = Vec::new();
    let mut after = None;
    loop {
        let page = database.query(&everything(), after, 2).unwrap();
        seen.extend(page.visits.into_iter().map(|v| v.url));
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }

    let mut expected: Vec<String> = (0..7).map(|index| format!("https://{index}.test")).collect();
    expected.reverse();
    assert_eq!(seen, expected);
}

#[test]
fn the_last_page_has_no_cursor() {
    let database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();

    assert_eq!(database.query(&everything(), None, 1).unwrap().next, None);
}

#[test]
fn a_date_range_includes_its_start_and_excludes_its_end() {
    let database = database();
    for at in [9, 10, 19, 20] {
        database.record(visit(&format!("https://{at}.test"), "T", at)).unwrap();
    }

    let filter = HistoryFilter {
        from: Some(10),
        to: Some(20),
        ..everything()
    };

    assert_eq!(urls(&database, &filter), ["https://19.test", "https://10.test"]);
}

#[test]
fn transitions_narrow_to_how_a_visit_was_reached() {
    let database = database();
    database
        .record(visit_by("https://a.test", "A", 1, Transition::Typed))
        .unwrap();
    database
        .record(visit_by("https://b.test", "B", 2, Transition::Bookmark))
        .unwrap();
    database.record(visit("https://c.test", "C", 3)).unwrap();

    let filter = HistoryFilter {
        transitions: vec![Transition::Typed, Transition::Bookmark],
        ..everything()
    };

    assert_eq!(urls(&database, &filter), ["https://b.test", "https://a.test"]);
}

#[test]
fn text_finds_fragments_of_titles_and_addresses_ignoring_case() {
    let database = database();
    database
        .record(visit("https://github.com/x/issues/1", "Crash on start", 1))
        .unwrap();
    database.record(visit("https://b.test/", "Recipes", 2)).unwrap();

    assert_eq!(urls(&database, &text("ITHUB")), ["https://github.com/x/issues/1"]);
    assert_eq!(urls(&database, &text("/issues/")), ["https://github.com/x/issues/1"]);
    assert_eq!(urls(&database, &text("cipe")), ["https://b.test/"]);
}

#[test]
fn every_word_of_the_text_must_be_found() {
    let database = database();
    database.record(visit("https://a.test/", "Rust book", 1)).unwrap();
    database.record(visit("https://b.test/", "Rust blog", 2)).unwrap();

    assert_eq!(urls(&database, &text("rust  book")), ["https://a.test/"]);
}

#[test]
fn text_shorter_than_a_trigram_is_still_found() {
    let database = database();
    database.record(visit("https://a.test/", "Go tour", 1)).unwrap();
    database.record(visit("https://b.test/", "Other", 2)).unwrap();

    assert_eq!(urls(&database, &text("go")), ["https://a.test/"]);
}

#[test]
fn text_is_taken_literally() {
    let database = database();
    database
        .record(visit("https://a.test/", "100% \"real\" a_b", 1))
        .unwrap();
    database.record(visit("https://b.test/", "100 real ab", 2)).unwrap();

    assert_eq!(urls(&database, &text("%")), ["https://a.test/"]);
    assert_eq!(urls(&database, &text("_")), ["https://a.test/"]);
    assert_eq!(urls(&database, &text("\"real\"")), ["https://a.test/"]);
    assert_eq!(urls(&database, &text("OR")).len(), 0);
}

#[test]
fn included_sites_cover_their_subdomains() {
    let database = database();
    database.record(visit("https://mail.a.test/", "M", 1)).unwrap();
    database.record(visit("https://b.test/", "B", 2)).unwrap();
    database.record(visit("https://nota.test/", "N", 3)).unwrap();

    assert_eq!(
        urls(&database, &including(UrlPatternKind::Site, "a.test")),
        ["https://mail.a.test/"]
    );
}

#[test]
fn several_included_patterns_cover_any_of_them() {
    let database = database();
    database.record(visit("https://a.test/", "A", 1)).unwrap();
    database.record(visit("https://b.test/", "B", 2)).unwrap();
    database.record(visit("https://c.test/", "C", 3)).unwrap();

    let filter = HistoryFilter {
        include: vec![
            UrlPattern::new(UrlPatternKind::Site, "a.test"),
            UrlPattern::new(UrlPatternKind::Wildcard, "*//b.*"),
        ],
        ..everything()
    };

    assert_eq!(urls(&database, &filter), ["https://b.test/", "https://a.test/"]);
}

#[test]
fn excluded_patterns_leave_their_matches_out() {
    let database = database();
    database.record(visit("https://a.test/", "A", 1)).unwrap();
    database.record(visit("https://b.test/admin", "B", 2)).unwrap();

    let filter = HistoryFilter {
        exclude: vec![UrlPattern::new(UrlPatternKind::Regex, "/admin$")],
        ..everything()
    };

    assert_eq!(urls(&database, &filter), ["https://a.test/"]);
}

#[test]
fn every_field_of_a_filter_narrows_it_together() {
    let database = database();
    database
        .record(visit_by("https://a.test/rust", "Rust", 10, Transition::Typed))
        .unwrap();
    database.record(visit("https://a.test/rust2", "Rust", 11)).unwrap();
    database
        .record(visit_by("https://b.test/rust", "Rust", 12, Transition::Typed))
        .unwrap();
    database
        .record(visit_by("https://a.test/go", "Go", 13, Transition::Typed))
        .unwrap();
    database
        .record(visit_by("https://a.test/rust3", "Rust", 30, Transition::Typed))
        .unwrap();

    let filter = HistoryFilter {
        text: Some("rust".into()),
        from: Some(10),
        to: Some(20),
        include: vec![UrlPattern::new(UrlPatternKind::Site, "a.test")],
        exclude: vec![UrlPattern::new(UrlPatternKind::Page, "https://a.test/go")],
        transitions: vec![Transition::Typed],
    };

    assert_eq!(urls(&database, &filter), ["https://a.test/rust"]);
    assert_eq!(database.count(&filter).unwrap(), 1);
}

#[test]
fn a_filter_with_an_invalid_pattern_is_refused() {
    let database = database();

    let refused = database.query(&including(UrlPatternKind::Regex, "("), None, 10);

    assert!(matches!(refused, Err(HakuError::InvalidPattern(_))));
}

#[test]
fn counting_covers_exactly_what_the_filter_shows() {
    let database = database();
    database.record(visit("https://a.test/", "A", 1)).unwrap();
    database.record(visit("https://b.test/", "B", 2)).unwrap();
    database.record(visit("https://a.test/", "A", 3)).unwrap();

    assert_eq!(database.count(&everything()).unwrap(), 3);
    assert_eq!(database.count(&including(UrlPatternKind::Site, "a.test")).unwrap(), 2);
}

#[test]
fn sites_are_listed_most_visited_first_with_their_last_visit() {
    let database = database();
    database.record(visit("https://a.test/1", "A", 1)).unwrap();
    database.record(visit("https://b.test/", "B", 2)).unwrap();
    database.record(visit("https://a.test/2", "A", 3)).unwrap();
    database.record(visit("file:///C:/x.txt", "X", 4)).unwrap();

    let sites = database.sites(&everything(), 10).unwrap();

    assert_eq!(
        sites,
        [
            HistorySite {
                host: "a.test".into(),
                visits: 2,
                last_visited_at: 3
            },
            HistorySite {
                host: "b.test".into(),
                visits: 1,
                last_visited_at: 2
            },
        ]
    );
}

#[test]
fn deleting_by_filter_removes_exactly_what_the_filter_queries() {
    let database = database();
    for (index, host) in ["a.test", "b.test", "a.test", "c.test"].iter().enumerate() {
        database
            .record(visit(&format!("https://{host}/{index}"), "T", index as i64))
            .unwrap();
    }
    let filter = including(UrlPatternKind::Site, "a.test");
    let expected: Vec<String> = urls(&database, &everything())
        .into_iter()
        .filter(|url| !urls(&database, &filter).contains(url))
        .collect();

    let deleted = database.delete(&filter).unwrap();

    assert_eq!(deleted, 2);
    assert_eq!(urls(&database, &everything()), expected);
}

#[test]
fn deleting_some_visits_to_a_page_keeps_its_counts_in_step() {
    let database = database();
    database
        .record(visit_by("https://a.test", "A", 1, Transition::Typed))
        .unwrap();
    database.record(visit("https://b.test", "B", 2)).unwrap();
    database.record(visit("https://a.test", "A", 3)).unwrap();

    database
        .delete(&HistoryFilter {
            from: Some(3),
            ..everything()
        })
        .unwrap();

    assert_eq!(counts(&database, "https://a.test"), Some((1, 1, 1)));
}

#[test]
fn deleting_every_visit_to_a_page_forgets_the_page() {
    let database = database();
    database.record(visit("https://a.test", "Findable", 1)).unwrap();
    database.record(visit("https://b.test", "B", 2)).unwrap();

    database
        .delete(&including(UrlPatternKind::Page, "https://a.test"))
        .unwrap();

    assert_eq!(counts(&database, "https://a.test"), None);
    assert!(urls(&database, &text("Findable")).is_empty());
}

#[test]
fn deleting_chosen_visits_keeps_the_others() {
    let database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();
    database.record(visit("https://b.test", "B", 2)).unwrap();
    database.record(visit("https://a.test", "A", 3)).unwrap();
    let newest = all(&database)[0].id;

    database.delete_visits(&[newest, 9_999]).unwrap();

    assert_eq!(urls(&database, &everything()), ["https://b.test", "https://a.test"]);
    assert_eq!(counts(&database, "https://a.test"), Some((1, 0, 1)));
}

#[test]
fn deleting_everything_removes_every_visit_and_page() {
    let database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();

    assert_eq!(database.delete(&everything()).unwrap(), 1);

    assert!(all(&database).is_empty());
    assert_eq!(counts(&database, "https://a.test"), None);
}

#[test]
fn a_visit_recorded_after_deleting_starts_its_page_afresh() {
    let database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();
    database.delete(&everything()).unwrap();

    database.record(visit("https://a.test", "A", 2)).unwrap();

    assert_eq!(counts(&database, "https://a.test"), Some((1, 0, 2)));
}

#[test]
fn pruning_deletes_only_visits_before_the_cutoff() {
    let database = database();
    database.record(visit("https://a.test", "A", 1)).unwrap();
    database.record(visit("https://b.test", "B", 5)).unwrap();

    assert_eq!(database.prune(5).unwrap(), 1);

    assert_eq!(urls(&database, &everything()), ["https://b.test"]);
}

#[test]
fn clearing_history_keeps_downloads() {
    let database = database();
    database.save_download(&download(1, false)).unwrap();

    database.delete(&everything()).unwrap();

    assert_eq!(database.downloads().unwrap().len(), 1);
}

fn download(id: u64, private: bool) -> Download {
    Download {
        id: DownloadId(id),
        url: "https://a.test/file.zip".into(),
        path: r"C:\Users\me\Downloads\file.zip".into(),
        received: 40,
        total: Some(100),
        state: DownloadState::InProgress,
        failure: None,
        can_resume: true,
        started_at: id * 10,
        recent: true,
        private,
        slot: Some(crate::model::SlotId(2)),
    }
}

#[test]
fn a_stored_download_is_read_back_newest_first() {
    let database = database();
    database.save_download(&download(1, false)).unwrap();
    database.save_download(&download(2, false)).unwrap();

    let stored = database.downloads().unwrap();

    assert_eq!(
        stored.iter().map(|d| d.id).collect::<Vec<_>>(),
        [DownloadId(2), DownloadId(1)]
    );
    assert_eq!(stored[1].url, "https://a.test/file.zip");
    assert_eq!(stored[1].total, Some(100));
    assert_eq!(stored[1].state, DownloadState::InProgress);
    assert!(!stored[1].recent && stored[1].slot.is_none());
}

#[test]
fn a_negative_stored_download_value_is_refused_rather_than_wrapped() {
    let database = database();
    database.save_download(&download(1, false)).unwrap();
    database.connection.execute("UPDATE downloads SET id = -1", []).unwrap();

    assert!(database.downloads().is_err());
}

#[test]
fn saving_a_download_again_replaces_what_was_stored() {
    let database = database();
    let mut entry = download(1, false);
    database.save_download(&entry).unwrap();
    entry.state = DownloadState::Failed;
    entry.failure = Some(DownloadFailure::Network);
    database.save_download(&entry).unwrap();

    let stored = database.downloads().unwrap();

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].state, DownloadState::Failed);
    assert_eq!(stored[0].failure, Some(DownloadFailure::Network));
}

#[test]
fn a_private_download_is_never_written() {
    let database = database();
    database.save_download(&download(1, true)).unwrap();

    assert!(database.downloads().unwrap().is_empty());
    let rows: i64 = database
        .connection
        .query_row("SELECT COUNT(*) FROM downloads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn forgotten_downloads_are_removed_and_others_kept() {
    let database = database();
    database.save_download(&download(1, false)).unwrap();
    database.save_download(&download(2, false)).unwrap();

    database.forget_downloads(&[DownloadId(1)]).unwrap();

    let stored = database.downloads().unwrap();
    assert_eq!(stored.iter().map(|d| d.id).collect::<Vec<_>>(), [DownloadId(2)]);
}

#[test]
fn suggested_pages_have_every_word_and_come_most_used_first() {
    let database = database();
    database.record(visit("https://a.test/rust", "Rust book", 1)).unwrap();
    database
        .record(visit_by("https://b.test/rust", "Rust by example", 2, Transition::Typed))
        .unwrap();
    database.record(visit("https://c.test/", "Go book", 3)).unwrap();

    let pages = database.suggest_pages("RUST b", 10).unwrap();
    let found: Vec<(&str, i64)> = pages.iter().map(|page| (page.url.as_str(), page.typed_count)).collect();
    assert_eq!(found, [("https://b.test/rust", 1), ("https://a.test/rust", 0)]);
    assert!(database.suggest_pages("  ", 10).unwrap().is_empty());
}

#[test]
fn only_a_typed_host_completes() {
    let database = database();
    database.record(visit("https://www.youtube.com/watch", "", 1)).unwrap();
    assert_eq!(database.typed_host("you").unwrap(), None);

    database
        .record(visit_by("https://www.youtube.com/", "", 2, Transition::Typed))
        .unwrap();
    database
        .record(visit_by("https://you.test/", "", 3, Transition::Typed))
        .unwrap();
    database
        .record(visit_by("https://www.youtube.com/", "", 4, Transition::Typed))
        .unwrap();
    assert_eq!(database.typed_host("you").unwrap().as_deref(), Some("www.youtube.com"));
    assert_eq!(database.typed_host("you.").unwrap().as_deref(), Some("you.test"));
    assert_eq!(database.typed_host("%").unwrap(), None);
}
