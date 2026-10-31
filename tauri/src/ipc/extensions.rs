//! Installs, adds, updates and uninstalls the extensions in the extensions
//! folder, and tells every window's interface what is installed.
//! [Extensions](../../../docs/specs/extensions.md).
//!
//! Every change goes through [`AppState::extension_changes`], so installing
//! at startup and changes from settings never interleave over the folder or
//! `extensions.json`. All of it waits on the engine, so none of it may run on
//! the UI thread.

use std::path::{Path, PathBuf};
use std::sync::{MutexGuard, PoisonError};
use std::time::Duration;

use tauri::Manager;
use tauri_specta::Event;

use crate::error::{HakuError, Result};
use crate::model::{
    data_url, default_locale, display_name, folder_name, icon_file, id_from_key, key, options_url, popup_url, version,
    AddedExtension, Extension, ExtensionSource, Extensions, FailedExtension,
};
use crate::platform;
use crate::state::{now_ms, AppState};
use crate::storage::{self, package, InstalledExtension, InstalledExtensions};
use crate::webview;

use super::events::ExtensionsChanged;

/// How often, and how far apart, moving or deleting a folder is tried while a
/// file in it is still held open, as the engine may hold one for a moment
/// after an extension is switched off or removed.
const FOLDER_ATTEMPTS: u32 = 10;
const FOLDER_RETRY: Duration = Duration::from_millis(100);

fn lock(state: &AppState) -> MutexGuard<'_, ()> {
    // The guard protects no data of its own, only the order of changes.
    state.extension_changes.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Installs every extension in the extensions folder into the shared profile,
/// switches each on or off as the settings say, and uninstalls the ones Haku
/// installed before whose folder is gone.
///
/// Only ids Haku recorded installing are uninstalled: the engine lists its own
/// built-in extensions alongside the user's, with nothing to tell them apart,
/// and refuses to remove those. One that fails to uninstall stays recorded, to
/// be tried again at the next start.
///
/// Runs once, at startup and off the UI thread: the engine has to be asked
/// and waited on for each extension.
///
/// A failure is kept in [`Extensions::error`] for the settings page to show,
/// since nothing is waiting on startup to report it.
pub fn sync(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let extensions = {
        let _turn = lock(&state);
        let extensions = install_all(app, &state).unwrap_or_else(|error| Extensions {
            error: Some(error),
            ..Extensions::default()
        });
        if let Ok(mut stored) = state.extensions.write() {
            *stored = extensions.clone();
        }
        extensions
    };
    webview::extensions_installed(app);
    let _ = ExtensionsChanged(extensions).emit(app);
}

/// # Errors
/// Fails when the chrome webview cannot be reached. A folder that does not
/// install is reported in [`Extensions::failed`], and any other step that
/// fails in [`Extensions::error`], without stopping the rest.
fn install_all(app: &tauri::AppHandle, state: &AppState) -> Result<Extensions> {
    let chrome = webview::chrome(app)?;
    let root = &state.paths.extensions;
    let mut problems = Vec::new();

    let mut installed = Vec::new();
    let mut now = Vec::new();
    let mut failed = Vec::new();
    let mut folders: Vec<PathBuf> = match std::fs::read_dir(root) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter(|path| {
                // Left over from a change that did not finish, or a folder
                // that could not be deleted at once: never an extension.
                if folder_of(path).starts_with('.') {
                    let _ = std::fs::remove_dir_all(path);
                    return false;
                }
                true
            })
            .collect(),
        // No folder yet means no extensions.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            problems.push(HakuError::from(error));
            Vec::new()
        }
    };
    // The order the file system lists folders in is not stable.
    folders.sort();
    for folder in folders {
        match platform::install_extension(&chrome, &folder) {
            Ok((id, name)) => {
                now.push(InstalledExtension {
                    id: id.clone(),
                    folder: folder_of(&folder),
                });
                installed.push(listed(id, name, &folder));
            }
            Err(error) => failed.push(FailedExtension {
                folder: folder_of(&folder),
                error,
            }),
        }
    }

    let disabled = disabled(state)?;
    for extension in &installed {
        let enabled = !disabled.contains(&extension.id);
        if let Err(error) = platform::set_extension_enabled(&chrome, extension.id.clone(), enabled) {
            problems.push(error);
        }
    }

    let before: InstalledExtensions = storage::read_json(&state.paths.installed_extensions);
    let mut recorded: Vec<InstalledExtension> = before
        .installed
        .iter()
        .filter(|old| !now.iter().any(|new| new.id == old.id))
        .cloned()
        .collect();
    // An extension with a `key` keeps its id when its folder moves: still
    // installed, from its new folder.
    let gone = before
        .removed(|folder| root.join(folder).is_dir())
        .into_iter()
        .filter(|old| !now.iter().any(|new| new.id == old.id));
    for gone in gone {
        match platform::uninstall_extension(&chrome, gone.id.clone()) {
            Ok(()) => recorded.retain(|kept| kept.id != gone.id),
            Err(error) => problems.push(error),
        }
    }
    recorded.extend(now);
    if let Err(error) = storage::write_json(
        &state.paths.installed_extensions,
        &InstalledExtensions { installed: recorded },
    ) {
        problems.push(error);
    }

    Ok(Extensions {
        installed,
        failed,
        error: problems.into_iter().next(),
    })
}

/// Asks the user for an extension, then adds it, or updates the one already
/// installed that it is a version of. Answers nothing when the picker was
/// cancelled.
///
/// # Errors
/// Fails when what was picked is not an extension, the engine refuses it, or
/// the folder of the version it replaces is in use. Nothing is changed then.
pub fn add(app: &tauri::AppHandle, window: &tauri::Window, source: ExtensionSource) -> Result<Option<AddedExtension>> {
    let Some(picked) = platform::pick_extension(window, source)? else {
        return Ok(None);
    };
    let state = app.state::<AppState>();
    let added = {
        let _turn = lock(&state);
        let root = &state.paths.extensions;
        std::fs::create_dir_all(root)?;
        let staging = root.join(format!(".adding-{}", now_ms()));
        let unpacked = match source {
            ExtensionSource::File => std::fs::read(&picked)
                .map_err(HakuError::from)
                .and_then(|bytes| package::unpack(&bytes, &staging)),
            ExtensionSource::Folder => package::copy_folder(&picked, &staging),
        };
        let added = unpacked.and_then(|()| place(app, &state, &staging));
        // Gone already when it took the extension's place.
        let _ = std::fs::remove_dir_all(&staging);
        added?
    };
    announce(app);
    Ok(Some(added))
}

/// Puts the extension unpacked in `staging` in its folder and installs it,
/// replacing the version installed there, if any, and putting that back if
/// the engine refuses the new one.
fn place(app: &tauri::AppHandle, state: &AppState, staging: &Path) -> Result<AddedExtension> {
    let chrome = webview::chrome(app)?;
    let root = &state.paths.extensions;
    let manifest = std::fs::read_to_string(staging.join("manifest.json"))?;
    let messages = default_locale(&manifest)
        .and_then(|locale| std::fs::read_to_string(staging.join("_locales").join(locale).join("messages.json")).ok());
    let name = display_name(&manifest, messages.as_deref()).unwrap_or_default();
    let key_id = key(&manifest)
        .and_then(|key| package::decode_key(&key))
        .map(|bytes| id_from_key(&bytes));

    let installed = read(state)?.installed;
    let previous = key_id
        .as_ref()
        .and_then(|id| installed.iter().find(|extension| &extension.id == id))
        .or_else(|| {
            let folder = folder_name(&name);
            installed.iter().find(|extension| extension.folder == folder)
        })
        .cloned();
    let folder = previous
        .as_ref()
        .map_or_else(|| folder_name(&name), |previous| previous.folder.clone());
    let target = root.join(&folder);
    let disabled = disabled(state)?;

    // Switched off first, so the engine lets go of its files.
    if let Some(previous) = &previous {
        let _ = platform::set_extension_enabled(&chrome, previous.id.clone(), false);
    }
    let restore_previous = || {
        if let Some(previous) = &previous {
            let enabled = !disabled.contains(&previous.id);
            let _ = platform::set_extension_enabled(&chrome, previous.id.clone(), enabled);
        }
    };
    let aside = root.join(format!(".replaced-{}", now_ms()));
    let set_aside = target.exists();
    if set_aside {
        if let Err(error) = retrying(|| std::fs::rename(&target, &aside)) {
            restore_previous();
            return Err(HakuError::Storage(format!(
                "{} is in use, so it was not updated: {error}",
                target.display()
            )));
        }
    }
    let put_back = || {
        let _ = std::fs::remove_dir_all(&target);
        if set_aside {
            let _ = std::fs::rename(&aside, &target);
            if previous.is_some() {
                let _ = platform::install_extension(&chrome, &target);
            }
        }
        restore_previous();
    };
    if let Err(error) = std::fs::rename(staging, &target) {
        put_back();
        return Err(error.into());
    }
    let (id, engine_name) = match platform::install_extension(&chrome, &target) {
        Ok(installed) => installed,
        Err(error) => {
            put_back();
            return Err(error);
        }
    };
    let mut problems = Vec::new();
    if let Err(error) = platform::set_extension_enabled(&chrome, id.clone(), !disabled.contains(&id)) {
        problems.push(error);
    }
    let _ = std::fs::remove_dir_all(&aside);

    // Its data is kept under its id, so an id that changed leaves the old one
    // installed with nothing to install it from.
    let replaced_id = previous
        .as_ref()
        .map(|previous| previous.id.clone())
        .filter(|old| *old != id);
    if let Some(old) = &replaced_id {
        if let Err(error) = platform::uninstall_extension(&chrome, old.clone()) {
            problems.push(error);
        }
    }

    let extension = listed(id.clone(), engine_name, &target);
    record(state, |recorded| {
        recorded.retain(|entry| entry.folder != folder && entry.id != id && Some(&entry.id) != replaced_id.as_ref());
        recorded.push(InstalledExtension {
            id: id.clone(),
            folder: folder.clone(),
        });
    })?;
    update(state, |extensions| {
        extensions
            .installed
            .retain(|entry| entry.folder != folder && entry.id != id && Some(&entry.id) != replaced_id.as_ref());
        extensions.failed.retain(|failed| failed.folder != folder);
        extensions.installed.push(extension.clone());
        extensions.installed.sort_by(|a, b| a.folder.cmp(&b.folder));
        if let Some(error) = problems.into_iter().next() {
            extensions.error = Some(error);
        }
    })?;
    Ok(AddedExtension {
        extension,
        replaced: previous.map(|previous| previous.version),
    })
}

/// Uninstalls the extension in `folder`, with what it stored, and deletes the
/// folder; or deletes a folder that failed to install.
///
/// # Errors
/// Fails when `folder` is neither, the engine refuses, or the folder can be
/// neither deleted nor set aside, in which case it comes back at the next
/// start.
pub fn remove(app: &tauri::AppHandle, folder: &str) -> Result<()> {
    let state = app.state::<AppState>();
    let outcome = {
        let _turn = lock(&state);
        let extensions = read(&state)?;
        let id = extensions
            .installed
            .iter()
            .find(|extension| extension.folder == folder)
            .map(|extension| extension.id.clone());
        if id.is_none() && !extensions.failed.iter().any(|failed| failed.folder == folder) {
            return Err(HakuError::Unsupported(format!(
                "no extension is installed from {folder}"
            )));
        }
        if let Some(id) = &id {
            platform::uninstall_extension(&webview::chrome(app)?, id.clone())?;
        }

        record(&state, |recorded| recorded.retain(|entry| entry.folder != folder))?;
        update(&state, |extensions| {
            extensions.installed.retain(|extension| extension.folder != folder);
            extensions.failed.retain(|failed| failed.folder != folder);
        })?;

        // Listed by the state, which is only ever filled from the folder's
        // own entries, so this stays inside the extensions folder.
        let path = state.paths.extensions.join(folder);
        let aside = state.paths.extensions.join(format!(".removed-{}", now_ms()));
        retrying(|| match std::fs::remove_dir_all(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            deleted => deleted,
        })
        .or_else(|_| retrying(|| std::fs::rename(&path, &aside)))
        .map_err(|error| {
            HakuError::Storage(format!(
                "{} could not be deleted, and comes back at the next start: {error}",
                path.display()
            ))
        })
    };
    announce(app);
    outcome
}

/// What the interface lists of the extension installed as `id` from `folder`.
fn listed(id: String, name: String, folder: &Path) -> Extension {
    let manifest = std::fs::read_to_string(folder.join("manifest.json")).unwrap_or_default();
    let icon = icon_file(&manifest).and_then(|(path, mime)| {
        std::fs::read(folder.join(path))
            .ok()
            .map(|bytes| data_url(mime, &bytes))
    });
    Extension {
        popup: popup_url(&id, &manifest),
        options: options_url(&id, &manifest),
        version: version(&manifest),
        folder: folder_of(folder),
        icon,
        id,
        name,
    }
}

fn folder_of(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn disabled(state: &AppState) -> Result<Vec<String>> {
    Ok(state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?
        .disabled_extensions
        .clone())
}

fn read(state: &AppState) -> Result<Extensions> {
    Ok(state
        .extensions
        .read()
        .map_err(|_| HakuError::Storage("extensions lock poisoned".into()))?
        .clone())
}

fn update(state: &AppState, change: impl FnOnce(&mut Extensions)) -> Result<()> {
    let mut extensions = state
        .extensions
        .write()
        .map_err(|_| HakuError::Storage("extensions lock poisoned".into()))?;
    change(&mut extensions);
    Ok(())
}

fn record(state: &AppState, change: impl FnOnce(&mut Vec<InstalledExtension>)) -> Result<()> {
    let mut recorded: InstalledExtensions = storage::read_json(&state.paths.installed_extensions);
    change(&mut recorded.installed);
    storage::write_json(&state.paths.installed_extensions, &recorded)
}

/// Tells every window what is installed now.
fn announce(app: &tauri::AppHandle) {
    if let Ok(extensions) = read(&app.state::<AppState>()) {
        let _ = ExtensionsChanged(extensions).emit(app);
    }
}

fn retrying(mut attempt: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut tries = 1;
    loop {
        match attempt() {
            Ok(()) => return Ok(()),
            Err(error) if tries >= FOLDER_ATTEMPTS || error.kind() == std::io::ErrorKind::NotFound => {
                return Err(error)
            }
            Err(_) => {
                tries += 1;
                std::thread::sleep(FOLDER_RETRY);
            }
        }
    }
}
