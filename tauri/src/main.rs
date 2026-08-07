//! The ROM Manager desktop application (issue #34).
//!
//! # This file is deliberately thin
//!
//! Every command below does the same three things: lock the session, call one
//! method, map the error to a string. There is no logic here, and there should
//! never be any — anything decided in this file would be a rule the test suite
//! cannot reach, because the suites drive [`Session`] directly with a fake
//! transport and never construct a Tauri application at all.
//!
//! # The webview's whole surface is these eight commands
//!
//! No plugin is depended on: not `fs`, not `sql`, not `shell`, not `http`. That
//! matters more than the capability file, which only *withholds* permissions
//! from plugins that are present. A permission cannot be re-granted by editing
//! JSON when the code that would honour it was never compiled in.
//!
//! Notice also what the commands take: identifiers and a boolean. No path, no
//! query, no URL. A compromised or replaced frontend cannot express a request
//! that reaches past this boundary, because the vocabulary to express one does
//! not exist.

// Windows: no console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use rom_manager::{AppPaths, FilesystemTransport, Library, Session, Snapshot, Store};
use tauri::{Manager, State};

/// The session, behind a lock because Tauri dispatches commands concurrently.
///
/// Serializing them is correct rather than merely convenient: two sync
/// operations against one device at once is not a thing this application should
/// ever attempt, and the lock makes that unrepresentable instead of a race
/// nobody notices until a card is half-written.
struct AppState {
    session: Mutex<Session<FilesystemTransport>>,
}

type Reply = Result<Snapshot, String>;

/// The lock, with a message rather than a panic if it is poisoned.
///
/// A poisoned lock means a previous command panicked mid-operation. Continuing
/// would mean acting on state whose invariants are unknown, so every command
/// refuses from then on.
macro_rules! session {
    ($state:expr) => {
        $state
            .session
            .lock()
            .map_err(|_| "the application is in an inconsistent state".to_owned())?
    };
}

/// A command taking nothing but the session.
macro_rules! plain_command {
    ($name:ident) => {
        #[tauri::command]
        fn $name(state: State<'_, AppState>) -> Reply {
            session!(state).$name().map_err(|error| error.to_string())
        }
    };
}

/// Scans every remembered folder and gathers what it finds into ROM Packs.
///
/// The scan itself is reported through the returned state rather than as a
/// list, so the UI shows what is now choosable instead of a log of files.
#[tauri::command]
fn scan_import_folders(state: State<'_, AppState>) -> Reply {
    let mut session = session!(state);
    session
        .scan_all_import_folders()
        .map_err(|error| error.to_string())?;
    session.load_snapshot().map_err(|error| error.to_string())
}

plain_command!(load_snapshot);
plain_command!(refresh_target);
plain_command!(build_plan);
plain_command!(request_cancellation);
plain_command!(dismiss_result);

#[tauri::command]
fn select_rom_pack(state: State<'_, AppState>, rom_pack_id: String, revision: u32) -> Reply {
    session!(state)
        .select_rom_pack(&rom_pack_id, revision)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn select_media_target(state: State<'_, AppState>, target_id: String) -> Reply {
    session!(state)
        .select_media_target(&target_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn initialize_target(state: State<'_, AppState>, confirmed: bool) -> Reply {
    session!(state)
        .initialize_target(confirmed)
        .map_err(|error| error.to_string())
}

/// Opens a native folder picker and remembers the choice as a Media Target.
///
/// The command takes **no arguments**. That is the whole point: the webview
/// says "the user wants to add a device", and the operating system's own
/// picker — not the frontend — determines which directory that is. A path
/// never crosses the boundary in either direction, so the "commands take
/// identifiers, never paths" rule survives having a file picker at all.
#[tauri::command]
fn pick_media_target(state: State<'_, AppState>) -> Reply {
    let Some(directory) = rfd::FileDialog::new()
        .set_title("Choose the device or card to sync to")
        .pick_folder()
    else {
        // Cancelling is not an error. Returning the unchanged state means the
        // UI simply carries on rather than showing a failure for a decision
        // the user is entitled to make.
        return session!(state).load_snapshot().map_err(|e| e.to_string());
    };
    let label = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| directory.to_string_lossy().into_owned());

    let mut session = session!(state);
    session
        .nominate_media_target(&directory.to_string_lossy(), &label)
        .map_err(|error| error.to_string())?;
    session.load_snapshot().map_err(|error| error.to_string())
}

/// Opens a native folder picker and remembers the choice as an Import Folder.
///
/// Remembering a folder is not scanning it. Nothing is read here; the
/// application never walks the user's disks on its own schedule.
#[tauri::command]
fn pick_import_folder(state: State<'_, AppState>) -> Reply {
    let Some(directory) = rfd::FileDialog::new()
        .set_title("Choose a folder to look for ROMs in")
        .pick_folder()
    else {
        return session!(state).load_snapshot().map_err(|e| e.to_string());
    };
    let mut session = session!(state);
    session
        .nominate_import_folder(&directory.to_string_lossy())
        .map_err(|error| error.to_string())?;
    session.load_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn approve_and_execute(
    state: State<'_, AppState>,
    plan_digest: String,
    acknowledged_removals: usize,
) -> Reply {
    session!(state)
        .approve_and_execute(&plan_digest, acknowledged_removals)
        .map_err(|error| error.to_string())
}

/// Opens durable state where the platform says it belongs.
///
/// A failure here is fatal and says so. Continuing with an in-memory store
/// would give the user an application that appears to work and forgets
/// everything when it closes, which is worse than not starting.
fn open_store() -> Result<Store, String> {
    let paths = AppPaths::from_env()
        .ok_or_else(|| "this system does not report a home directory".to_owned())?;
    if let Some(parent) = paths.database().parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    Store::open(&paths.database()).map_err(|error| format!("{error}"))
}

fn main() {
    let paths = match AppPaths::from_env() {
        Some(paths) => paths,
        None => {
            eprintln!("ROM Manager cannot start: this system reports no home directory");
            std::process::exit(1);
        }
    };
    let store = match open_store() {
        Ok(store) => store,
        Err(reason) => {
            eprintln!("ROM Manager cannot start: {reason}");
            std::process::exit(1);
        }
    };

    // The catalogues come from durable state, so whatever the user nominated
    // last time is there this time. Nothing is invented: a fresh install offers
    // nothing until a folder or a device is picked.
    let connect = Box::new(|locator: &str| {
        FilesystemTransport::new(locator).map_err(|error| error.to_string())
    });
    let mut session = Session::new(store, connect);
    match Library::open(paths.library_root()) {
        Ok(library) => session.set_library(library),
        Err(error) => {
            // Without storage nothing can be imported, and an application that
            // silently could not take anything in would look broken in a way
            // the user could not diagnose.
            eprintln!("ROM Manager cannot start: the Library is unusable: {error}");
            std::process::exit(1);
        }
    }

    tauri::Builder::default()
        .setup(move |app| {
            app.manage(AppState {
                session: Mutex::new(session),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_snapshot,
            select_rom_pack,
            select_media_target,
            pick_media_target,
            pick_import_folder,
            scan_import_folders,
            initialize_target,
            refresh_target,
            build_plan,
            approve_and_execute,
            request_cancellation,
            dismiss_result,
        ])
        .run(tauri::generate_context!())
        .expect("the desktop application starts");
}
