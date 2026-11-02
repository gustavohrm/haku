//! Opening Haku's SQLite files: creating, migrating, and setting a damaged
//! one aside.

use std::fs;
use std::path::Path;

use rusqlite::{Connection, ErrorCode, Transaction};

use crate::error::Result;
use crate::storage::corrupt_path;

/// Suffixes of the SQLite files that belong to the main database file.
const COMPANION_SUFFIXES: &[&str] = &["", "-wal", "-shm"];

/// One schema revision.
///
/// Most are SQL, written as a plain `&str`. A revision that needs Rust to
/// compute what it stores, such as a host parsed from each URL, is a
/// [`Migration::Rust`].
pub trait Step {
    /// Applies the revision inside `transaction`.
    ///
    /// # Errors
    /// Returns the failure of any statement the revision runs.
    fn apply(&self, transaction: &Transaction) -> rusqlite::Result<()>;
}

impl Step for &str {
    fn apply(&self, transaction: &Transaction) -> rusqlite::Result<()> {
        transaction.execute_batch(self)
    }
}

/// A revision written as SQL or as a Rust function.
pub enum Migration {
    Sql(&'static str),
    Rust(fn(&Transaction) -> rusqlite::Result<()>),
}

impl Step for Migration {
    fn apply(&self, transaction: &Transaction) -> rusqlite::Result<()> {
        match self {
            Self::Sql(sql) => transaction.execute_batch(sql),
            Self::Rust(function) => function(transaction),
        }
    }
}

/// How long a statement waits for a lock another connection holds.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// Opens the database at `path`, creating its folder, and applies
/// `migrations` it has not seen yet.
///
/// # Errors
/// Returns [`crate::error::HakuError::Storage`] when the folder cannot be
/// created, the file cannot be opened, or a migration fails.
pub fn open(path: &Path, migrations: &[impl Step]) -> Result<Connection> {
    create_parent(path)?;
    Ok(connect(path, migrations)?)
}

/// Opens the database at `path` as [`open`] does, setting a damaged file
/// aside first.
///
/// A file SQLite cannot read as a database is moved to its
/// [`corrupt_path`], with its `-wal` and `-shm` files, and a fresh database
/// takes its place. Any other failure, such as a locked or unreadable file,
/// is returned without touching the file.
///
/// # Errors
/// Returns [`crate::error::HakuError::Storage`] when the file cannot be
/// opened, moved aside, or a migration fails.
pub fn open_or_set_aside(path: &Path, migrations: &[impl Step]) -> Result<Connection> {
    create_parent(path)?;
    match connect(path, migrations) {
        Ok(connection) => Ok(connection),
        Err(error) if is_damage(&error) => {
            set_aside(path)?;
            Ok(connect(path, migrations)?)
        }
        Err(error) => Err(error.into()),
    }
}

/// An in-memory database with `migrations` applied.
///
/// # Errors
/// Returns [`crate::error::HakuError::Storage`] when a migration fails.
pub fn in_memory(migrations: &[impl Step]) -> Result<Connection> {
    let mut connection = Connection::open_in_memory()?;
    migrate(&mut connection, migrations)?;
    Ok(connection)
}

/// Opens the file at `path`, which must exist or be creatable, and migrates it.
pub fn connect(path: &Path, migrations: &[impl Step]) -> rusqlite::Result<Connection> {
    let mut connection = Connection::open(path)?;
    // A write by another Haku holds the file for milliseconds; waiting
    // briefly is better than failing a write, or startup, over it.
    connection.busy_timeout(BUSY_TIMEOUT)?;
    migrate(&mut connection, migrations)?;
    Ok(connection)
}

/// Applies every migration the database has not seen yet.
///
/// `user_version` is used as the revision counter because it costs no table
/// and SQLite maintains it as part of the file header. Migrations are
/// append-only: never edit a shipped entry, add a new one.
pub fn migrate(connection: &mut Connection, migrations: &[impl Step]) -> rusqlite::Result<()> {
    let version: usize = connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as usize;

    for (index, migration) in migrations.iter().enumerate().skip(version) {
        let transaction = connection.transaction()?;
        migration.apply(&transaction)?;
        transaction.pragma_update(None, "user_version", (index + 1) as i64)?;
        transaction.commit()?;
    }
    Ok(())
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
