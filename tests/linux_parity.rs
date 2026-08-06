//! Coverage for Linux application-data placement and filesystem support
//! (issues #74, #75).

use std::{collections::HashMap, path::Path};

use rom_manager::{AppPaths, FilesystemSupport, ObservedFilesystem, fits, support_for};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

#[test]
fn paths_default_to_the_xdg_fallbacks() {
    let paths = AppPaths::resolve(env(&[("HOME", "/home/andy")])).unwrap();

    assert_eq!(paths.data, Path::new("/home/andy/.local/share/rom-manager"));
    assert_eq!(paths.cache, Path::new("/home/andy/.cache/rom-manager"));
    assert_eq!(paths.config, Path::new("/home/andy/.config/rom-manager"));
}

#[test]
fn paths_honour_the_xdg_variables_when_set() {
    let paths = AppPaths::resolve(env(&[
        ("HOME", "/home/andy"),
        ("XDG_DATA_HOME", "/data"),
        ("XDG_CACHE_HOME", "/scratch"),
        ("XDG_CONFIG_HOME", "/settings"),
    ]))
    .unwrap();

    assert_eq!(paths.data, Path::new("/data/rom-manager"));
    assert_eq!(paths.cache, Path::new("/scratch/rom-manager"));
    assert_eq!(paths.config, Path::new("/settings/rom-manager"));
}

#[test]
fn an_empty_variable_falls_back_rather_than_yielding_a_root_path() {
    let paths = AppPaths::resolve(env(&[("HOME", "/home/andy"), ("XDG_DATA_HOME", "")])).unwrap();

    assert_eq!(paths.data, Path::new("/home/andy/.local/share/rom-manager"));
}

#[test]
fn the_library_lives_in_data_and_the_cache_lives_in_cache() {
    // The placement *is* the guarantee. A cleaner deleting the cache directory
    // must cost time, never content.
    let paths = AppPaths::resolve(env(&[("HOME", "/home/andy")])).unwrap();

    assert!(paths.library_root().starts_with(&paths.data));
    assert!(paths.database().starts_with(&paths.data));
    assert!(paths.materialization_cache().starts_with(&paths.cache));

    assert!(
        !paths.materialization_cache().starts_with(&paths.data),
        "a routine disk cleanup must never be able to reach the Library"
    );
}

#[test]
fn appimage_mount_points_are_recognised_as_ephemeral() {
    // Anything written there is lost when the process exits.
    assert!(AppPaths::is_ephemeral_mount(Path::new(
        "/tmp/.mount_romman1a2b3c/usr/bin"
    )));
    assert!(AppPaths::is_ephemeral_mount(Path::new(
        "/run/user/1000/appimage"
    )));
    assert!(!AppPaths::is_ephemeral_mount(Path::new(
        "/home/andy/.local/share/rom-manager"
    )));
}

#[test]
fn supported_filesystems_are_accepted() {
    for reported in ["ext4", "exFAT", "NTFS", "ntfs3"] {
        let filesystem = ObservedFilesystem::parse(reported);
        assert!(
            support_for(&filesystem, false).is_supported(),
            "{reported} should be supported"
        );
    }
}

#[test]
fn fat32_is_rejected_with_a_reason_the_user_can_read() {
    for reported in ["fat32", "vfat", "msdos"] {
        let filesystem = ObservedFilesystem::parse(reported);
        assert_eq!(filesystem, ObservedFilesystem::Fat32);

        let support = support_for(&filesystem, false);
        assert!(!support.is_supported());
        let reason = support.reason().unwrap();
        assert!(reason.contains("4 GiB"), "the size ceiling is named");
        assert!(reason.contains("case"), "the case-handling risk is named");
    }
}

#[test]
fn fat32_can_be_opted_into_knowingly() {
    // Opting in changes nothing about the risk; it records that the user was
    // told.
    let support = support_for(&ObservedFilesystem::Fat32, true);
    assert!(support.is_supported());
}

#[test]
fn an_undeterminable_filesystem_is_blocked() {
    // "We could not tell" is not evidence of safety.
    let support = support_for(&ObservedFilesystem::parse(""), false);

    assert!(!support.is_supported());
    assert!(
        support
            .reason()
            .unwrap()
            .contains("could not be determined")
    );
}

#[test]
fn an_unqualified_filesystem_is_blocked_by_name() {
    let filesystem = ObservedFilesystem::parse("btrfs");
    assert_eq!(filesystem, ObservedFilesystem::Unqualified("btrfs".into()));

    let support = support_for(&filesystem, false);
    assert!(!support.is_supported());
    assert!(support.reason().unwrap().contains("btrfs"));
}

#[test]
fn fat32_size_ceiling_is_checked_at_planning_time() {
    // The point of knowing the limit is to catch it *before* transferring.
    let fat32 = ObservedFilesystem::Fat32;

    assert!(fits(&fat32, 3 * 1024 * 1024 * 1024));
    assert!(!fits(&fat32, 5 * 1024 * 1024 * 1024));

    // Filesystems without a practical ceiling never block on size.
    assert!(fits(&ObservedFilesystem::Ext4, 64 * 1024 * 1024 * 1024));
    assert!(fits(&ObservedFilesystem::ExFat, 64 * 1024 * 1024 * 1024));
}

#[test]
fn a_blocked_filesystem_reports_rather_than_failing_later() {
    // The rejection carries the observed filesystem, so a plan can show it.
    let support = support_for(&ObservedFilesystem::Fat32, false);

    assert!(matches!(support, FilesystemSupport::Blocked { .. }));
    assert!(support.reason().is_some());
}

#[test]
fn macos_paths_follow_apples_convention() {
    // The same guarantee as XDG, expressed in the platform's terms.
    let paths = AppPaths::resolve_macos(env(&[("HOME", "/Users/andy")])).unwrap();

    assert_eq!(
        paths.data,
        Path::new("/Users/andy/Library/Application Support/dev.mill1893.rom-manager")
    );
    assert_eq!(
        paths.cache,
        Path::new("/Users/andy/Library/Caches/dev.mill1893.rom-manager")
    );
    assert_eq!(
        paths.config,
        Path::new("/Users/andy/Library/Preferences/dev.mill1893.rom-manager")
    );
}

#[test]
fn the_macos_cache_can_never_reach_the_library() {
    // macOS purges Caches under disk pressure without asking. Putting the
    // Library there would let the operating system delete the user's content.
    let paths = AppPaths::resolve_macos(env(&[("HOME", "/Users/andy")])).unwrap();

    assert!(paths.library_root().starts_with(&paths.data));
    assert!(paths.materialization_cache().starts_with(&paths.cache));
    assert!(!paths.materialization_cache().starts_with(&paths.data));
}
