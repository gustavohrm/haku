//! Where Haku keeps what it must remember between runs.
//!
//! Two stores, chosen per shape of the data. Settings and the open-tab session
//! are small, read whole, and worth being able to inspect or hand-edit, so they
//! are JSON files. History grows without bound and will need filtering and
//! ranged queries, so it is SQLite.

pub mod history_db;
pub mod merge;
pub mod session;
pub mod settings;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::{HakuError, Result};

pub use history_db::HistoryDb;
pub use session::{Session, SessionWindow, WindowBounds};
pub use settings::Settings;

/// Reads a JSON document, layering it over the type's defaults.
///
/// Missing keys keep their default, so a hand-edited file naming one setting
/// does not reset the rest. A corrupt or unreadable file yields the defaults
/// outright: losing a window layout is a far better outcome than refusing to
/// start.
pub fn read_json<T: Serialize + DeserializeOwned + Default>(path: &Path) -> T {
    let defaults = T::default();
    let Ok(base) = serde_json::to_value(&defaults) else {
        return defaults;
    };

    let stored = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let Some(stored) = stored else {
        return defaults;
    };

    serde_json::from_value(merge::merge(base, stored)).unwrap_or(defaults)
}

/// Serialises every document write.
///
/// The session is saved from commands and from page reports, which run on
/// different threads. Two writers sharing one temporary file could interleave
/// and leave a corrupt document, which loses every open tab on the next launch.
static WRITES: Mutex<()> = Mutex::new(());

/// Writes a JSON document atomically.
///
/// The write goes to a temporary file that then replaces the target, so a crash
/// mid-write leaves the previous contents intact instead of a truncated file.
/// Writes are serialised, so concurrent saves land one after another.
///
/// # Errors
/// Returns [`HakuError::Storage`] when the directory cannot be created or either
/// filesystem step fails.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(value).map_err(|error| HakuError::Storage(error.to_string()))?;

    // A writer that panicked left nothing half-done that the guard protects.
    let _guard = WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, body)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

/// Filesystem locations Haku writes to, resolved once at startup.
#[derive(Clone, Debug)]
pub struct Paths {
    pub settings: PathBuf,
    pub session: PathBuf,
    pub history: PathBuf,
    /// One unpacked Chrome extension per subfolder, installed at startup.
    pub extensions: PathBuf,
}

impl Paths {
    pub fn under(root: &Path) -> Self {
        Self {
            settings: root.join("settings.json"),
            session: root.join("session.json"),
            history: root.join("history.sqlite"),
            extensions: root.join("extensions"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Sample {
        value: u32,
    }

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Pair {
        value: u32,
        other: u32,
    }

    impl Default for Pair {
        fn default() -> Self {
            Self { value: 3, other: 4 }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("haku-storage-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reading_a_missing_file_yields_the_default_rather_than_failing() {
        let path = temp_dir("missing").join("absent.json");
        assert_eq!(read_json::<Sample>(&path), Sample::default());
    }

    #[test]
    fn a_document_missing_a_key_keeps_that_key_at_its_default() {
        let path = temp_dir("partial").join("partial.json");
        fs::write(&path, r#"{"other":7}"#).unwrap();

        assert_eq!(read_json::<Pair>(&path), Pair { value: 3, other: 7 });
    }

    #[test]
    fn reading_a_corrupt_file_yields_the_default_so_startup_still_succeeds() {
        let path = temp_dir("corrupt").join("broken.json");
        fs::write(&path, "{ not json").unwrap();

        assert_eq!(read_json::<Sample>(&path), Sample::default());
    }

    #[test]
    fn a_written_document_reads_back_unchanged() {
        let path = temp_dir("roundtrip").join("nested").join("value.json");
        write_json(&path, &Sample { value: 42 }).unwrap();

        assert_eq!(read_json::<Sample>(&path), Sample { value: 42 });
    }

    #[test]
    fn writing_leaves_no_temporary_file_behind() {
        let path = temp_dir("atomic").join("value.json");
        write_json(&path, &Sample { value: 1 }).unwrap();

        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn paths_are_derived_from_one_root_directory() {
        let paths = Paths::under(Path::new("/data"));

        assert!(paths.settings.ends_with("settings.json"));
        assert!(paths.session.ends_with("session.json"));
        assert!(paths.history.ends_with("history.sqlite"));
    }
}
