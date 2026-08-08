//! Seeds a Library from a real ROM folder, for working on the interface.
//!
//! Development scaffolding, not product code and not part of any milestone's
//! evidence. The Library Browser (#31) cannot be designed against an empty
//! database, and the only other way to fill one is to click through the app —
//! which is exactly what cannot be automated in this environment.
//!
//!     cargo run --example seed_library -- <scratch-home> <rom-folder>
//!
//! It builds the session the way `tauri/src/main.rs` does, so what lands in the
//! database is what the application itself would have put there. Nothing is
//! invented and nothing is written outside the scratch home it is given.

use std::path::PathBuf;

use rom_manager::{AppPaths, FilesystemTransport, Library, Session, Store};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(home), Some(folder)) = (args.next(), args.next()) else {
        eprintln!("usage: seed_library <scratch-home> <rom-folder>");
        std::process::exit(2);
    };

    // Resolve exactly as the application does, against a HOME we were handed
    // rather than the caller's own. This is the whole isolation guarantee.
    let paths = AppPaths::resolve(|name| match name {
        "HOME" => Some(home.clone()),
        _ => None,
    })
    .expect("a home directory was supplied");

    let database = paths.database();
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).expect("scratch data directory is creatable");
    }

    let store = Store::open(&database).expect("store opens");
    let connect =
        Box::new(|locator: &str| FilesystemTransport::new(locator).map_err(|e| e.to_string()));
    let mut session = Session::new(store, connect);
    session.set_library(Library::open(paths.library_root()).expect("library opens"));

    let folder = PathBuf::from(&folder);
    assert!(folder.is_dir(), "{} is not a directory", folder.display());

    println!("seeding {} from {}", database.display(), folder.display());
    session
        .nominate_import_folder(&folder.to_string_lossy())
        .expect("folder is remembered");

    session
        .scan_all_import_folders()
        .expect("the scan completes");

    let snapshot = session.load_snapshot().expect("a snapshot is produced");
    match snapshot.last_scan {
        Some(scan) => {
            println!(
                "scanned {} folder(s): {} ROM Sets added, {} declined",
                scan.folders_scanned,
                scan.rom_sets_added,
                scan.declined.len()
            );
            let mut by_code = std::collections::BTreeMap::<String, (usize, String, String)>::new();
            for file in &scan.declined {
                let entry = by_code.entry(file.code.clone()).or_insert((
                    0,
                    file.remediation.clone(),
                    file.path.clone(),
                ));
                entry.0 += 1;
            }
            for (code, (count, remediation, example)) in by_code {
                println!("  {count:>5}  {code}");
                println!("         {remediation}");
                println!("         e.g. {example}");
            }
        }
        None => println!("the scan reported nothing"),
    }
}
