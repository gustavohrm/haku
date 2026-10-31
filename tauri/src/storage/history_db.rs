//! Durable browsing history.
//!
//! SQLite rather than a JSON file because history is the one store that grows
//! without limit and will be queried by range, host and text once the advanced
//! history feature lands. Visits are recorded from the first release so that
//! feature has data to show when it arrives.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};
use crate::model::{Download, DownloadFailure, DownloadId, DownloadState};
use crate::storage::corrupt_path;

/// Suffixes of the SQLite files that belong to the main database file.
const COMPANION_SUFFIXES: &[&str] = &["", "-wal", "-shm"];

/// How long a statement waits for a lock another connection holds.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// Schema revisions, applied in order to whatever version a database is on.
///
/// Migrations are append-only: never edit a shipped entry, add a new one.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE visits (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        url        TEXT    NOT NULL,
        title      TEXT    NOT NULL,
        visited_at INTEGER NOT NULL
    );
    CREATE INDEX idx_visits_visited_at ON visits (visited_at DESC);
    CREATE INDEX idx_visits_url ON visits (url);",
    // Downloads share the file with history, as in Chrome: both are records
    // of what was fetched, they grow without limit, and they are kept as long.
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
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Exported as a JavaScript number: every value here is a counter, a
    /// small capacity, or a millisecond timestamp, all far inside the range a
    /// double represents exactly.
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    pub url: String,
    pub title: String,
    /// Milliseconds since the Unix epoch.
    #[specta(type = specta_typescript::Number)]
    pub visited_at: i64,
}

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
        create_parent(path)?;
        Ok(Self::connect(path)?)
    }

    /// Opens the database at `path`, setting a damaged file aside first.
    ///
    /// A file SQLite cannot read as a database is moved to `history.sqlite.corrupt`,
    /// with its `-wal` and `-shm` files, and a fresh database takes its place.
    /// Any other failure, such as a locked or unreadable file, is returned
    /// without touching the file.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the file cannot be
    /// opened, moved aside, or a migration fails.
    pub fn open_or_set_aside(path: &Path) -> Result<Self> {
        create_parent(path)?;
        match Self::connect(path) {
            Ok(database) => Ok(database),
            Err(error) if is_damage(&error) => {
                set_aside(path)?;
                Ok(Self::connect(path)?)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Opens the database at `path` as [`Self::open_or_set_aside`] does, or
    /// keeps history in memory for this run when the file cannot be opened.
    ///
    /// The browser is still usable without its history file, and Haku has no
    /// window yet to say why it would not start. A file another Haku holds
    /// locked, or one that cannot be read, is left untouched for the next run,
    /// except by [`Self::clear`]. The failure is returned alongside, for the
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

    fn connect(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        // A write by another Haku holds the file for milliseconds; waiting
        // briefly is better than failing a visit, or startup, over it.
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let mut database = Self {
            connection,
            unopened: None,
        };
        database.migrate()?;
        Ok(database)
    }

    /// In-memory database, for tests.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when migration fails.
    pub fn in_memory() -> Result<Self> {
        let mut database = Self {
            connection: Connection::open_in_memory()?,
            unopened: None,
        };
        database.migrate()?;
        Ok(database)
    }

    /// Applies every migration the database has not seen yet.
    ///
    /// `user_version` is used as the revision counter because it costs no table
    /// and SQLite maintains it as part of the file header.
    fn migrate(&mut self) -> rusqlite::Result<()> {
        let version: usize = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as usize;

        for (index, migration) in MIGRATIONS.iter().enumerate().skip(version) {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(migration)?;
            transaction.pragma_update(None, "user_version", (index + 1) as i64)?;
            transaction.commit()?;
        }
        Ok(())
    }

    /// Records a visit.
    ///
    /// Pages report themselves repeatedly as they load, so an immediate repeat
    /// of the same URL updates the existing row instead of adding a duplicate.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the write fails.
    pub fn record(&self, url: &str, title: &str, visited_at: i64) -> Result<()> {
        let previous: Option<(i64, String)> = self
            .connection
            .query_row("SELECT id, url FROM visits ORDER BY id DESC LIMIT 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .ok();

        if let Some((id, previous_url)) = previous {
            if previous_url == url {
                self.connection.execute(
                    "UPDATE visits SET title = ?1, visited_at = ?2 WHERE id = ?3",
                    params![title, visited_at, id],
                )?;
                return Ok(());
            }
        }

        self.connection.execute(
            "INSERT INTO visits (url, title, visited_at) VALUES (?1, ?2, ?3)",
            params![url, title, visited_at],
        )?;
        Ok(())
    }

    /// Updates the title of the most recent visit to `url`.
    ///
    /// Pages set or change their title after they arrive, and keep changing it
    /// while open. The visit should carry the latest title without each change
    /// counting as another visit. A URL never visited is left alone.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the write fails.
    pub fn retitle(&self, url: &str, title: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE visits SET title = ?1 WHERE id = (SELECT MAX(id) FROM visits WHERE url = ?2)",
            params![title, url],
        )?;
        Ok(())
    }

    /// Most recent visits first.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the query fails.
    pub fn recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, url, title, visited_at FROM visits ORDER BY visited_at DESC, id DESC LIMIT ?1")?;

        let rows = statement.query_map([limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                visited_at: row.get(3)?,
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Removes every visit.
    ///
    /// While history is kept in memory, the file it could not be opened from
    /// is cleared too, or the visits in it would come back on the next run.
    ///
    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the delete fails, or
    /// when the file history could not be opened from still cannot be.
    pub fn clear(&self) -> Result<()> {
        self.connection.execute("DELETE FROM visits", [])?;
        if let Some(path) = &self.unopened {
            Self::connect(path)?.clear()?;
        }
        Ok(())
    }
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
        let rows = statement.query_map([], |row| {
            let state: String = row.get(5)?;
            let failure: Option<String> = row.get(6)?;
            Ok(Download {
                id: DownloadId(row.get::<_, i64>(0)? as u64),
                url: row.get(1)?,
                path: row.get(2)?,
                received: row.get::<_, i64>(3)? as u64,
                total: row.get::<_, Option<i64>>(4)?.map(|total| total as u64),
                state: state_from(&state),
                failure: failure.as_deref().map(failure_from),
                can_resume: false,
                started_at: row.get::<_, i64>(7)? as u64,
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

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Whether SQLite reports the file as something other than a usable database.
/// Other failures, such as a lock held by another process, say nothing about
/// the file's contents.
fn is_damage(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(failure.code, ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
    )
}

/// Moves the database file and its companions aside, replacing earlier copies.
fn set_aside(path: &Path) -> Result<()> {
    for suffix in COMPANION_SUFFIXES {
        let mut companion = path.as_os_str().to_owned();
        companion.push(suffix);
        let companion = Path::new(&companion);
        if companion.exists() {
            fs::rename(companion, corrupt_path(companion))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> HistoryDb {
        HistoryDb::in_memory().unwrap()
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("haku-history-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("history.sqlite")
    }

    #[test]
    fn a_damaged_file_is_set_aside_and_a_fresh_database_opens_in_its_place() {
        let path = temp_path("damaged");
        fs::write(&path, "this is not a database, just bytes\n").unwrap();

        let database = HistoryDb::open_or_set_aside(&path).unwrap();

        assert!(database.recent(10).unwrap().is_empty());
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
        database.record("https://a.test", "A", 1).unwrap();

        assert_eq!(database.recent(10).unwrap().len(), 1);
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
            .record("https://a.test", "A", 1)
            .unwrap();
        let holder = Connection::open(&path).unwrap();
        holder.execute_batch("BEGIN EXCLUSIVE").unwrap();

        let (database, _) = HistoryDb::open_or_in_memory(&path);
        database.record("https://b.test", "B", 2).unwrap();
        assert_eq!(database.recent(10).unwrap().len(), 1);
        assert!(database.clear().is_err());

        holder.execute_batch("ROLLBACK").unwrap();
        database.clear().unwrap();
        assert!(HistoryDb::open(&path).unwrap().recent(10).unwrap().is_empty());
    }

    #[test]
    fn a_healthy_database_is_opened_as_it_is_and_not_moved() {
        let path = temp_path("healthy");
        HistoryDb::open(&path)
            .unwrap()
            .record("https://a.test", "A", 1)
            .unwrap();

        let database = HistoryDb::open_or_set_aside(&path).unwrap();

        assert_eq!(database.recent(10).unwrap().len(), 1);
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
        database.record("https://a.test", "A", 1).unwrap();

        database.migrate().unwrap();

        assert_eq!(database.recent(10).unwrap().len(), 1);
    }

    #[test]
    fn a_recorded_visit_is_read_back() {
        let database = database();
        database.record("https://a.test", "A", 1_700_000_000_000).unwrap();

        let entries = database.recent(10).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://a.test");
        assert_eq!(entries[0].visited_at, 1_700_000_000_000);
    }

    #[test]
    fn reloading_the_same_page_updates_the_visit_rather_than_duplicating_it() {
        let database = database();
        database.record("https://a.test", "Loading", 1).unwrap();
        database.record("https://a.test", "Ready", 2).unwrap();

        let entries = database.recent(10).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Ready");
        assert_eq!(entries[0].visited_at, 2);
    }

    #[test]
    fn returning_to_a_page_after_visiting_another_records_a_new_visit() {
        let database = database();
        database.record("https://a.test", "A", 1).unwrap();
        database.record("https://b.test", "B", 2).unwrap();
        database.record("https://a.test", "A", 3).unwrap();

        assert_eq!(database.recent(10).unwrap().len(), 3);
    }

    #[test]
    fn recent_returns_the_newest_visits_first() {
        let database = database();
        database.record("https://a.test", "A", 1).unwrap();
        database.record("https://b.test", "B", 2).unwrap();

        let entries = database.recent(10).unwrap();

        assert_eq!(entries[0].url, "https://b.test");
    }

    #[test]
    fn recent_respects_its_limit() {
        let database = database();
        for index in 0..5 {
            database.record(&format!("https://{index}.test"), "T", index).unwrap();
        }

        assert_eq!(database.recent(2).unwrap().len(), 2);
    }

    #[test]
    fn retitling_updates_the_latest_visit_to_that_url_without_adding_one() {
        let database = database();
        database.record("https://a.test", "Old", 1).unwrap();
        database.record("https://b.test", "B", 2).unwrap();
        database.record("https://a.test", "Old", 3).unwrap();
        database.record("https://b.test", "B", 4).unwrap();

        database.retitle("https://a.test", "New").unwrap();

        let entries = database.recent(10).unwrap();
        assert_eq!(entries.len(), 4);
        let titles: Vec<&str> = entries
            .iter()
            .filter(|entry| entry.url == "https://a.test")
            .map(|entry| entry.title.as_str())
            .collect();
        assert_eq!(titles, vec!["New", "Old"]);
    }

    #[test]
    fn retitling_a_url_never_visited_records_nothing() {
        let database = database();

        database.retitle("https://a.test", "A").unwrap();

        assert!(database.recent(10).unwrap().is_empty());
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
    fn clearing_history_keeps_downloads() {
        let database = database();
        database.save_download(&download(1, false)).unwrap();

        database.clear().unwrap();

        assert_eq!(database.downloads().unwrap().len(), 1);
    }

    #[test]
    fn a_database_from_before_downloads_is_migrated_with_its_visits() {
        let mut database = HistoryDb {
            connection: Connection::open_in_memory().unwrap(),
            unopened: None,
        };
        database.connection.execute_batch(MIGRATIONS[0]).unwrap();
        database.connection.pragma_update(None, "user_version", 1).unwrap();
        database.record("https://a.test", "A", 1).unwrap();

        database.migrate().unwrap();

        assert_eq!(database.recent(10).unwrap().len(), 1);
        database.save_download(&download(1, false)).unwrap();
        assert_eq!(database.downloads().unwrap().len(), 1);
    }

    #[test]
    fn clearing_removes_every_visit() {
        let database = database();
        database.record("https://a.test", "A", 1).unwrap();

        database.clear().unwrap();

        assert!(database.recent(10).unwrap().is_empty());
    }
}
