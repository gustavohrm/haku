//! Durable browsing history, and the downloads list stored beside it.
//! [History § Storage](../../../docs/specs/history.md#storage).
//!
//! SQLite rather than a JSON file because history is the one store that grows
//! without limit and is queried by range, host and text.

use std::path::{Path, PathBuf};

use rusqlite::functions::FunctionFlags;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};
use crate::model::{
    any_matches, host_of, CompiledPattern, Download, DownloadFailure, DownloadId, DownloadState, Transition, UrlPattern,
};
use crate::storage::sqlite::{self, Migration};

/// Schema revisions, applied in order by [`sqlite::open`].
const MIGRATIONS: &[Migration] = &[
    Migration::Sql(
        "CREATE TABLE visits (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        url        TEXT    NOT NULL,
        title      TEXT    NOT NULL,
        visited_at INTEGER NOT NULL
    );
    CREATE INDEX idx_visits_visited_at ON visits (visited_at DESC);
    CREATE INDEX idx_visits_url ON visits (url);",
    ),
    // Downloads share the file with history, as in Chrome: both are records
    // of what was fetched, they grow without limit, and they are kept as long.
    Migration::Sql(
        "CREATE TABLE downloads (
        id         INTEGER PRIMARY KEY,
        url        TEXT    NOT NULL,
        path       TEXT    NOT NULL,
        received   INTEGER NOT NULL,
        total      INTEGER,
        state      TEXT    NOT NULL,
        failure    TEXT,
        started_at INTEGER NOT NULL
    );",
    ),
    Migration::Rust(pages_and_transitions),
];

/// Pages, with the counts the address bar ranks by, and visits that point at
/// them with how they were reached. Every earlier visit is kept, as `other`.
const PAGES: &str = "
    CREATE TABLE pages (
        id              INTEGER PRIMARY KEY,
        url             TEXT    NOT NULL UNIQUE,
        host            TEXT    NOT NULL,
        title           TEXT    NOT NULL,
        visit_count     INTEGER NOT NULL,
        typed_count     INTEGER NOT NULL,
        last_visited_at INTEGER NOT NULL
    );
    INSERT INTO pages (url, host, title, visit_count, typed_count, last_visited_at)
        SELECT url, '',
               (SELECT latest.title FROM visits latest WHERE latest.url = visits.url
                ORDER BY latest.visited_at DESC, latest.id DESC LIMIT 1),
               COUNT(*), 0, MAX(visited_at)
        FROM visits GROUP BY url;
    CREATE TABLE page_visits (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        page_id    INTEGER NOT NULL REFERENCES pages (id),
        visited_at INTEGER NOT NULL,
        transition TEXT    NOT NULL
    );
    INSERT INTO page_visits (id, page_id, visited_at, transition)
        SELECT visits.id, pages.id, visits.visited_at, 'other'
        FROM visits JOIN pages ON pages.url = visits.url;
    DROP TABLE visits;
    ALTER TABLE page_visits RENAME TO visits;
    CREATE INDEX idx_visits_visited_at ON visits (visited_at DESC, id DESC);
    CREATE INDEX idx_visits_page ON visits (page_id);
    CREATE INDEX idx_pages_host ON pages (host);";

/// Text search over titles and addresses. Trigrams, because addresses are
/// searched by fragments that word tokenizers miss.
const PAGES_SEARCH: &str = "
    CREATE VIRTUAL TABLE pages_fts USING fts5 (
        title, url, content = 'pages', content_rowid = 'id', tokenize = 'trigram'
    );
    CREATE TRIGGER pages_fts_insert AFTER INSERT ON pages BEGIN
        INSERT INTO pages_fts (rowid, title, url) VALUES (new.id, new.title, new.url);
    END;
    CREATE TRIGGER pages_fts_delete AFTER DELETE ON pages BEGIN
        INSERT INTO pages_fts (pages_fts, rowid, title, url) VALUES ('delete', old.id, old.title, old.url);
    END;
    CREATE TRIGGER pages_fts_update AFTER UPDATE OF title, url ON pages BEGIN
        INSERT INTO pages_fts (pages_fts, rowid, title, url) VALUES ('delete', old.id, old.title, old.url);
        INSERT INTO pages_fts (rowid, title, url) VALUES (new.id, new.title, new.url);
    END;
    INSERT INTO pages_fts (pages_fts) VALUES ('rebuild');";

/// The third revision. Rust, because each page's host is parsed from its URL
/// as [`host_of`] parses it everywhere else, which SQL cannot do reliably.
fn pages_and_transitions(transaction: &Transaction) -> rusqlite::Result<()> {
    transaction.execute_batch(PAGES)?;
    let pages: Vec<(i64, String)> = transaction
        .prepare("SELECT id, url FROM pages")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut update = transaction.prepare("UPDATE pages SET host = ?1 WHERE id = ?2")?;
    for (id, url) in pages {
        update.execute(params![host_of(&url).unwrap_or_default(), id])?;
    }
    transaction.execute_batch(PAGES_SEARCH)
}

/// The shortest search term the trigram index can find; shorter ones are
/// matched by scanning.
const TRIGRAM: usize = 3;

/// The most visits one query returns.
pub const QUERY_LIMIT_MAX: u32 = 500;

/// Which visits a query, count or delete covers. Every field narrows it; the
/// default covers everything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilter {
    /// Words found in the title or address, each anywhere, ignoring case.
    pub text: Option<String>,
    /// Milliseconds since the epoch, inclusive.
    #[specta(type = Option<specta_typescript::Number>)]
    pub from: Option<i64>,
    /// Milliseconds since the epoch, exclusive.
    #[specta(type = Option<specta_typescript::Number>)]
    pub to: Option<i64>,
    /// Visits whose URL any of these picks out; none means every URL.
    pub include: Vec<UrlPattern>,
    /// Visits whose URL any of these picks out are left out.
    pub exclude: Vec<UrlPattern>,
    /// Visits reached in any of these ways; none means every way.
    pub transitions: Vec<Transition>,
}

impl HistoryFilter {
    /// Whether the filter covers every visit.
    pub fn is_everything(&self) -> bool {
        self.terms().is_empty()
            && self.from.is_none()
            && self.to.is_none()
            && self.include.is_empty()
            && self.exclude.is_empty()
            && self.transitions.is_empty()
    }

    fn terms(&self) -> Vec<&str> {
        self.text
            .as_deref()
            .map(|text| text.split_whitespace().collect())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryVisit {
    /// Exported as a JavaScript number: every value here is a counter or a
    /// millisecond timestamp, far inside the range a double represents
    /// exactly.
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    pub url: String,
    /// Empty for a URL with no host, such as a local file.
    pub host: String,
    /// The page's latest title; empty when a rule keeps only its address.
    pub title: String,
    /// Milliseconds since the Unix epoch.
    #[specta(type = specta_typescript::Number)]
    pub visited_at: i64,
    pub transition: Transition,
}

/// Where the next page of a query starts: after the visit it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCursor {
    #[specta(type = specta_typescript::Number)]
    pub visited_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub visits: Vec<HistoryVisit>,
    /// Nothing when no visit is left.
    pub next: Option<HistoryCursor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistorySite {
    pub host: String,
    #[specta(type = specta_typescript::Number)]
    pub visits: i64,
    #[specta(type = specta_typescript::Number)]
    pub last_visited_at: i64,
}

/// A visit to record.
#[derive(Clone, Copy, Debug)]
pub struct NewVisit<'a> {
    pub url: &'a str,
    pub title: &'a str,
    /// Milliseconds since the Unix epoch.
    pub visited_at: i64,
    pub transition: Transition,
}

/// A filter as SQL over `visits v JOIN pages p`, with its parameters.
struct Clause {
    sql: String,
    values: Vec<Value>,
}

const FROM_VISITS: &str = "FROM visits v JOIN pages p ON p.id = v.page_id";

pub struct HistoryDb {
    connection: Connection,
    /// The file history could not be opened from, while it is kept in memory
    /// instead.
    unopened: Option<PathBuf>,
}

impl HistoryDb {
    /// Opens the database at `path`, creating and migrating it as needed.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the file cannot be
    /// opened or a migration fails.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self::wrap(sqlite::open(path, MIGRATIONS)?))
    }

    /// Opens the database at `path`, setting a damaged file aside first, as
    /// [`sqlite::open_or_set_aside`] does.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the file cannot be
    /// opened, moved aside, or a migration fails.
    pub fn open_or_set_aside(path: &Path) -> Result<Self> {
        Ok(Self::wrap(sqlite::open_or_set_aside(path, MIGRATIONS)?))
    }

    /// Opens the database at `path` as [`Self::open_or_set_aside`] does, or
    /// keeps history in memory for this run when the file cannot be opened.
    ///
    /// The browser is still usable without its history file, and Haku has no
    /// window yet to say why it would not start. A file another Haku holds
    /// locked, or one that cannot be read, is left untouched for the next run,
    /// except by [`Self::delete`]. The failure is returned alongside, for the
    /// user to be told once a window can show it.
    ///
    /// # Panics
    /// Panics if SQLite cannot create an in-memory database with the fixed
    /// schema, which leaves nothing to fall back to.
    pub fn open_or_in_memory(path: &Path) -> (Self, Option<HakuError>) {
        match Self::open_or_set_aside(path) {
            Ok(database) => (database, None),
            Err(error) => (
                Self {
                    unopened: Some(path.to_path_buf()),
                    ..Self::in_memory().expect("an in-memory history database could not be created")
                },
                Some(HakuError::Storage(format!(
                    "history could not be opened, so this run's visits will not be kept: {error}"
                ))),
            ),
        }
    }

    fn wrap(connection: Connection) -> Self {
        Self {
            connection,
            unopened: None,
        }
    }

    /// In-memory database, for tests.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when migration fails.
    pub fn in_memory() -> Result<Self> {
        Ok(Self::wrap(sqlite::in_memory(MIGRATIONS)?))
    }

    /// Records a visit.
    ///
    /// Pages report themselves repeatedly as they load, so an immediate repeat
    /// of the same URL updates the existing visit instead of adding another.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the write fails.
    pub fn record(&self, visit: NewVisit<'_>) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let latest: Option<(i64, i64, String)> = transaction
            .query_row(
                "SELECT v.id, p.id, p.url FROM visits v JOIN pages p ON p.id = v.page_id ORDER BY v.id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        match latest {
            Some((visit_id, page_id, url)) if url == visit.url => {
                transaction.execute(
                    "UPDATE visits SET visited_at = ?1 WHERE id = ?2",
                    params![visit.visited_at, visit_id],
                )?;
                transaction.execute(
                    "UPDATE pages SET title = ?1, last_visited_at = MAX(last_visited_at, ?2) WHERE id = ?3",
                    params![visit.title, visit.visited_at, page_id],
                )?;
            }
            _ => {
                let typed = i64::from(visit.transition == Transition::Typed);
                transaction.execute(
                    "INSERT INTO pages (url, host, title, visit_count, typed_count, last_visited_at)
                     VALUES (?1, ?2, ?3, 1, ?4, ?5)
                     ON CONFLICT (url) DO UPDATE SET
                         title = excluded.title,
                         visit_count = visit_count + 1,
                         typed_count = typed_count + excluded.typed_count,
                         last_visited_at = MAX(last_visited_at, excluded.last_visited_at)",
                    params![
                        visit.url,
                        host_of(visit.url).unwrap_or_default(),
                        visit.title,
                        typed,
                        visit.visited_at
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO visits (page_id, visited_at, transition)
                     VALUES ((SELECT id FROM pages WHERE url = ?1), ?2, ?3)",
                    params![visit.url, visit.visited_at, visit.transition.name()],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Updates the title of a visited page.
    ///
    /// Pages set or change their title after they arrive, and keep changing it
    /// while open. The page should carry the latest title without each change
    /// counting as another visit. A URL never visited is left alone.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the write fails.
    pub fn retitle(&self, url: &str, title: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE pages SET title = ?1 WHERE url = ?2 AND title <> ?1",
            params![title, url],
        )?;
        Ok(())
    }

    /// Visits the filter covers, newest first, starting after `after`.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::InvalidPattern`] for a pattern that
    /// cannot be used, or [`crate::error::HakuError::Storage`] when the query
    /// fails.
    pub fn query(&self, filter: &HistoryFilter, after: Option<HistoryCursor>, limit: u32) -> Result<HistoryPage> {
        let limit = limit.clamp(1, QUERY_LIMIT_MAX);
        let Clause { mut sql, mut values } = self.clause(filter)?;
        if let Some(cursor) = after {
            sql.push_str(" AND (v.visited_at < ? OR (v.visited_at = ? AND v.id < ?))");
            values.extend([cursor.visited_at, cursor.visited_at, cursor.id].map(Value::Integer));
        }
        values.push(Value::Integer(i64::from(limit) + 1));
        let mut statement = self.connection.prepare(&format!(
            "SELECT v.id, p.url, p.host, p.title, v.visited_at, v.transition {FROM_VISITS} WHERE {sql}
             ORDER BY v.visited_at DESC, v.id DESC LIMIT ?"
        ))?;
        let mut visits = statement
            .query_map(params_from_iter(values), |row| {
                Ok(HistoryVisit {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    host: row.get(2)?,
                    title: row.get(3)?,
                    visited_at: row.get(4)?,
                    transition: Transition::from_name(&row.get::<_, String>(5)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let more = visits.len() > limit as usize;
        visits.truncate(limit as usize);
        let next = more.then(|| visits.last()).flatten().map(|last| HistoryCursor {
            visited_at: last.visited_at,
            id: last.id,
        });
        Ok(HistoryPage { visits, next })
    }

    /// How many visits the filter covers.
    ///
    /// # Errors
    /// As [`Self::query`].
    pub fn count(&self, filter: &HistoryFilter) -> Result<i64> {
        let Clause { sql, values } = self.clause(filter)?;
        Ok(self.connection.query_row(
            &format!("SELECT COUNT(*) {FROM_VISITS} WHERE {sql}"),
            params_from_iter(values),
            |row| row.get(0),
        )?)
    }

    /// The hosts of the visits the filter covers, most visited first.
    ///
    /// # Errors
    /// As [`Self::query`].
    pub fn sites(&self, filter: &HistoryFilter, limit: u32) -> Result<Vec<HistorySite>> {
        let Clause { sql, mut values } = self.clause(filter)?;
        values.push(Value::Integer(i64::from(limit.max(1))));
        let mut statement = self.connection.prepare(&format!(
            "SELECT p.host, COUNT(*), MAX(v.visited_at) {FROM_VISITS} WHERE {sql} AND p.host <> ''
             GROUP BY p.host ORDER BY COUNT(*) DESC, MAX(v.visited_at) DESC, p.host LIMIT ?"
        ))?;
        let sites = statement
            .query_map(params_from_iter(values), |row| {
                Ok(HistorySite {
                    host: row.get(0)?,
                    visits: row.get(1)?,
                    last_visited_at: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sites)
    }

    /// Deletes the visits the filter covers.
    ///
    /// While history is kept in memory, the same visits are deleted from the
    /// file it could not be opened from, or they would come back on the next
    /// run.
    ///
    /// @returns How many visits were deleted here.
    ///
    /// # Errors
    /// As [`Self::query`], and [`crate::error::HakuError::Storage`] when the
    /// file history could not be opened from still cannot be.
    pub fn delete(&self, filter: &HistoryFilter) -> Result<i64> {
        let deleted = self.delete_here(filter)?;
        if let Some(path) = &self.unopened {
            Self::wrap(sqlite::connect(path, MIGRATIONS)?).delete_here(filter)?;
        }
        Ok(deleted)
    }

    /// Deletes visits older than `before`, here only: the file a run in
    /// memory could not open is pruned when a run opens it.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the delete fails.
    pub fn prune(&self, before: i64) -> Result<i64> {
        self.delete_here(&HistoryFilter {
            to: Some(before),
            ..HistoryFilter::default()
        })
    }

    /// Deletes these visits; ids not stored are ignored.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the delete fails.
    pub fn delete_visits(&self, ids: &[i64]) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut pages = Vec::new();
        {
            let mut page_of = transaction.prepare("SELECT page_id FROM visits WHERE id = ?1")?;
            let mut delete = transaction.prepare("DELETE FROM visits WHERE id = ?1")?;
            for id in ids {
                if let Some(page) = page_of.query_row([id], |row| row.get::<_, i64>(0)).optional()? {
                    pages.push(page);
                    delete.execute([id])?;
                }
            }
        }
        refresh_pages(&transaction, &pages)?;
        transaction.commit()?;
        Ok(())
    }

    fn delete_here(&self, filter: &HistoryFilter) -> Result<i64> {
        let transaction = self.connection.unchecked_transaction()?;
        let deleted = if filter.is_everything() {
            let deleted = transaction.execute("DELETE FROM visits", [])?;
            transaction.execute("DELETE FROM pages", [])?;
            deleted
        } else {
            let Clause { sql, values } = self.clause(filter)?;
            let pages: Vec<i64> = transaction
                .prepare(&format!("SELECT DISTINCT v.page_id {FROM_VISITS} WHERE {sql}"))?
                .query_map(params_from_iter(values.clone()), |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let deleted = transaction.execute(
                &format!("DELETE FROM visits WHERE id IN (SELECT v.id {FROM_VISITS} WHERE {sql})"),
                params_from_iter(values),
            )?;
            refresh_pages(&transaction, &pages)?;
            deleted
        };
        transaction.commit()?;
        Ok(deleted as i64)
    }

    /// The filter as SQL, with its patterns registered as the functions it
    /// calls. Registered per call: the connection is used by one caller at a
    /// time, and the patterns change with every filter.
    fn clause(&self, filter: &HistoryFilter) -> Result<Clause> {
        let mut sql = String::from("1");
        let mut values = Vec::new();

        if let Some(from) = filter.from {
            sql.push_str(" AND v.visited_at >= ?");
            values.push(Value::Integer(from));
        }
        if let Some(to) = filter.to {
            sql.push_str(" AND v.visited_at < ?");
            values.push(Value::Integer(to));
        }
        if !filter.transitions.is_empty() {
            let marks = vec!["?"; filter.transitions.len()].join(", ");
            sql.push_str(&format!(" AND v.transition IN ({marks})"));
            values.extend(filter.transitions.iter().map(|t| Value::Text(t.name().into())));
        }

        let (long, short): (Vec<&str>, Vec<&str>) = filter
            .terms()
            .into_iter()
            .partition(|term| term.chars().count() >= TRIGRAM);
        if !long.is_empty() {
            let phrases: Vec<String> = long
                .iter()
                .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
                .collect();
            sql.push_str(" AND p.id IN (SELECT rowid FROM pages_fts WHERE pages_fts MATCH ?)");
            values.push(Value::Text(phrases.join(" ")));
        }
        for term in short {
            let like = format!(
                "%{}%",
                term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
            );
            sql.push_str(r" AND (p.title LIKE ? ESCAPE '\' OR p.url LIKE ? ESCAPE '\')");
            values.extend([Value::Text(like.clone()), Value::Text(like)]);
        }

        for (name, patterns, negate) in [
            ("haku_include", &filter.include, ""),
            ("haku_exclude", &filter.exclude, "NOT "),
        ] {
            if patterns.is_empty() {
                continue;
            }
            let compiled = patterns.iter().map(UrlPattern::compile).collect::<Result<Vec<_>>>()?;
            self.register(name, compiled)?;
            sql.push_str(&format!(" AND {negate}{name}(p.url, p.host)"));
        }

        Ok(Clause { sql, values })
    }

    fn register(&self, name: &str, patterns: Vec<CompiledPattern>) -> Result<()> {
        self.connection.create_scalar_function(
            name,
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            move |context| {
                let url: String = context.get(0)?;
                let host: String = context.get(1)?;
                Ok(any_matches(&patterns, &url, &host))
            },
        )?;
        Ok(())
    }
}

/// Brings each page's counts and last visit in step with the visits left,
/// and deletes a page left with none.
fn refresh_pages(transaction: &Transaction, pages: &[i64]) -> rusqlite::Result<()> {
    let mut update = transaction.prepare(
        "UPDATE pages SET
             visit_count = (SELECT COUNT(*) FROM visits WHERE page_id = ?1),
             typed_count = (SELECT COUNT(*) FROM visits WHERE page_id = ?1 AND transition = 'typed'),
             last_visited_at = COALESCE((SELECT MAX(visited_at) FROM visits WHERE page_id = ?1), last_visited_at)
         WHERE id = ?1",
    )?;
    let mut delete = transaction.prepare("DELETE FROM pages WHERE id = ?1 AND visit_count = 0")?;
    for page in pages {
        update.execute([page])?;
        delete.execute([page])?;
    }
    Ok(())
}

impl HistoryDb {
    /// Stores a download as it is now, replacing what was stored of it.
    ///
    /// A private download is never written, whoever asks.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the write fails.
    pub fn save_download(&self, download: &Download) -> Result<()> {
        if download.private {
            return Ok(());
        }
        self.connection.execute(
            "INSERT OR REPLACE INTO downloads (id, url, path, received, total, state, failure, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                download.id.0 as i64,
                download.url,
                download.path,
                download.received as i64,
                download.total.map(|total| total as i64),
                state_name(download.state),
                download.failure.map(failure_name),
                download.started_at as i64,
            ],
        )?;
        Ok(())
    }

    /// Every stored download, newest first.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the query fails.
    pub fn downloads(&self) -> Result<Vec<Download>> {
        let mut statement = self.connection.prepare(
            "SELECT id, url, path, received, total, state, failure, started_at
             FROM downloads ORDER BY started_at DESC, id DESC",
        )?;
        // A negative value is refused rather than wrapped, since a huge id
        // would overflow the next one.
        fn unsigned(column: usize, value: i64) -> rusqlite::Result<u64> {
            u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
        }
        let rows = statement.query_map([], |row| {
            let state: String = row.get(5)?;
            let failure: Option<String> = row.get(6)?;
            Ok(Download {
                id: DownloadId(unsigned(0, row.get(0)?)?),
                url: row.get(1)?,
                path: row.get(2)?,
                received: unsigned(3, row.get(3)?)?,
                total: row
                    .get::<_, Option<i64>>(4)?
                    .map(|total| unsigned(4, total))
                    .transpose()?,
                state: state_from(&state),
                failure: failure.as_deref().map(failure_from),
                can_resume: false,
                started_at: unsigned(7, row.get(7)?)?,
                recent: false,
                private: false,
                slot: None,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Forgets stored downloads, all or none. Their files stay.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the delete fails.
    pub fn forget_downloads(&self, ids: &[DownloadId]) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        {
            let mut statement = transaction.prepare("DELETE FROM downloads WHERE id = ?1")?;
            for id in ids {
                statement.execute([id.0 as i64])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

fn state_name(state: DownloadState) -> &'static str {
    match state {
        DownloadState::InProgress => "inProgress",
        DownloadState::Paused => "paused",
        DownloadState::Completed => "completed",
        DownloadState::Cancelled => "cancelled",
        DownloadState::Failed => "failed",
    }
}

/// An unknown name, written by a later Haku, reads as a failure.
fn state_from(name: &str) -> DownloadState {
    match name {
        "inProgress" => DownloadState::InProgress,
        "paused" => DownloadState::Paused,
        "completed" => DownloadState::Completed,
        "cancelled" => DownloadState::Cancelled,
        _ => DownloadState::Failed,
    }
}

fn failure_name(failure: DownloadFailure) -> &'static str {
    match failure {
        DownloadFailure::Network => "network",
        DownloadFailure::Server => "server",
        DownloadFailure::File => "file",
        DownloadFailure::Lost => "lost",
        DownloadFailure::Other => "other",
    }
}

fn failure_from(name: &str) -> DownloadFailure {
    match name {
        "network" => DownloadFailure::Network,
        "server" => DownloadFailure::Server,
        "file" => DownloadFailure::File,
        "lost" => DownloadFailure::Lost,
        _ => DownloadFailure::Other,
    }
}

#[cfg(test)]
#[path = "history_db_tests.rs"]
mod tests;
