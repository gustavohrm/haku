//! Where Haku keeps what it must remember between runs.
//!
//! Two stores, chosen per shape of the data. Settings and the open-tab session
//! are small, read whole, and worth being able to inspect or hand-edit, so they
//! are JSON files. History grows without bound and will need filtering and
//! ranged queries, so it is SQLite.

pub mod extensions;
pub mod history_db;
pub mod merge;
pub mod session;
pub mod settings;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::{HakuError, Result};

pub use extensions::{InstalledExtension, InstalledExtensions};
pub use history_db::HistoryDb;
pub use session::{Session, SessionWindow, WindowBounds};
pub use settings::Settings;

/// Suffix that moves an unreadable document aside.
const CORRUPT_SUFFIX: &str = ".corrupt";

/// Where a document that exists but cannot be read is moved, so the next save
/// does not overwrite it.
pub fn corrupt_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(CORRUPT_SUFFIX);
    PathBuf::from(name)
}

/// Reads a JSON document, layering it over the type's defaults.
///
/// Missing keys keep their default, so a hand-edited file naming one setting
/// does not reset the rest. A file that cannot be read yields the defaults
/// outright: losing a window layout is a far better outcome than refusing to
/// start. A file that exists but is not valid for the type is moved to
/// [`corrupt_path`] first, so the user's contents survive the next save.
pub fn read_json<T: Serialize + DeserializeOwned + Default>(path: &Path) -> T {
    let defaults = T::default();
    let Ok(base) = serde_json::to_value(&defaults) else {
        return defaults;
    };

    let Ok(raw) = fs::read_to_string(path) else {
        return defaults;
    };
    let document: Option<T> = serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|stored| serde_json::from_value(merge::merge(base, stored)).ok());
    match document {
        Some(document) => document,
        None => {
            let _ = fs::rename(path, corrupt_path(path));
            defaults
        }
    }
}

/// Serialises every document write.
///
/// The session is saved from commands and from page reports, which run on
/// different threads. Two writers sharing one temporary file could interleave
/// and leave a corrupt document, which loses every open tab on the next launch.
static WRITES: Mutex<()> = Mutex::new(());

/// Writes a JSON document atomically.
///
/// The write goes to a temporary file, flushed to disk before it replaces the
/// target, so a crash or power loss mid-write leaves the previous contents
/// intact instead of a truncated file. Writes are serialised, so concurrent
/// saves land one after another.
///
/// # Errors
/// Returns [`HakuError::Storage`] when the directory cannot be created or any
/// filesystem step fails.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(value).map_err(|error| HakuError::Storage(error.to_string()))?;

    // A writer that panicked left nothing half-done that the guard protects.
    let _guard = WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(body.as_bytes())?;
    file.sync_all()?;
    drop(file);
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
    /// The ids of the extensions Haku installed, so the ones whose folder
    /// is gone can be told from the engine's own.
    pub installed_extensions: PathBuf,
}

impl Paths {
    pub fn under(root: &Path) -> Self {
        Self {
            settings: root.join("settings.json"),
            session: root.join("session.json"),
            history: root.join("history.sqlite"),
            extensions: root.join("extensions"),
            installed_extensions: root.join("extensions.json"),
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
    fn a_corrupt_file_is_moved_aside_with_its_contents_intact() {
        let path = temp_dir("set-aside").join("broken.json");
        fs::write(&path, "{ not json").unwrap();

        read_json::<Sample>(&path);

        assert!(!path.exists());
        assert_eq!(fs::read_to_string(corrupt_path(&path)).unwrap(), "{ not json");
    }

    #[test]
    fn a_valid_file_is_not_moved_aside() {
        let path = temp_dir("keep-valid").join("value.json");
        write_json(&path, &Sample { value: 42 }).unwrap();

        read_json::<Sample>(&path);

        assert!(path.exists());
        assert!(!corrupt_path(&path).exists());
    }

    #[test]
    fn a_missing_file_creates_no_corrupt_file() {
        let path = temp_dir("no-trace").join("absent.json");

        read_json::<Sample>(&path);

        assert!(!corrupt_path(&path).exists());
    }

    #[test]
    fn a_valid_json_document_of_the_wrong_shape_is_moved_aside() {
        let path = temp_dir("wrong-shape").join("value.json");
        fs::write(&path, r#"{"value":"text"}"#).unwrap();

        assert_eq!(read_json::<Sample>(&path), Sample::default());
        assert!(!path.exists());
        assert!(corrupt_path(&path).exists());
    }

    #[test]
    fn a_new_corrupt_file_replaces_an_earlier_one_set_aside() {
        let path = temp_dir("replace-aside").join("broken.json");
        fs::write(corrupt_path(&path), "old").unwrap();
        fs::write(&path, "{ not json").unwrap();

        read_json::<Sample>(&path);

        assert_eq!(fs::read_to_string(corrupt_path(&path)).unwrap(), "{ not json");
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
