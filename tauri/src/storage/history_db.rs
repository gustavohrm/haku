//! Durable browsing history.
//!
//! SQLite rather than a JSON file because history is the one store that grows
//! without limit and will be queried by range, host and text once the advanced
//! history feature lands. Visits are recorded from the first release so that
//! feature has data to show when it arrives.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::Result;
use crate::storage::corrupt_path;

/// Suffixes of the SQLite files that belong to the main database file.
const COMPANION_SUFFIXES: &[&str] = &["", "-wal", "-shm"];

/// Schema revisions, applied in order to whatever version a database is on.
///
/// Migrations are append-only: never edit a shipped entry, add a new one.
const MIGRATIONS: &[&str] = &["CREATE TABLE visits (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        url        TEXT    NOT NULL,
        title      TEXT    NOT NULL,
        visited_at INTEGER NOT NULL
    );
    CREATE INDEX idx_visits_visited_at ON visits (visited_at DESC);
    CREATE INDEX idx_visits_url ON visits (url);"];

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

    fn connect(path: &Path) -> rusqlite::Result<Self> {
        let mut database = Self {
            connection: Connection::open(path)?,
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

    /// # Errors
    /// Returns [`crate::error::HakuError::Storage`] when the delete fails.
    pub fn clear(&self) -> Result<()> {
        self.connection.execute("DELETE FROM visits", [])?;
        Ok(())
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

    #[test]
    fn clearing_removes_every_visit() {
        let database = database();
        database.record("https://a.test", "A", 1).unwrap();

        database.clear().unwrap();

        assert!(database.recent(10).unwrap().is_empty());
    }
}
