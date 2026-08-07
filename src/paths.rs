//! Where the application keeps its things (issue #74).
//!
//! # Why the split matters
//!
//! The three directories are not decoration. They encode a promise about what
//! is safe to delete:
//!
//! - **Data** holds the Library and its database. Losing it loses the user's
//!   content, so nothing may remove it except the user, deliberately.
//! - **Cache** holds the Materialization Cache. Every cache-cleaning tool on a
//!   Linux desktop will eventually delete this directory without asking, and
//!   that must be *safe* — which is exactly why availability is never
//!   established by the cache.
//! - **Config** holds settings, which are cheap to recreate.
//!
//! Putting the cache under data would make a routine disk cleanup destroy the
//! Library. Putting the Library under cache would make it disposable. The
//! placement is the guarantee.
//!
//! # Never inside the AppImage
//!
//! An AppImage's mount point is read-only and vanishes when the process exits.
//! Anything written there is lost, so every path here is resolved from the
//! user's environment rather than from the executable's location.

use std::path::PathBuf;

/// The application's directories on this host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    /// Library content and the database. Never removed by an uninstall.
    pub data: PathBuf,
    /// Disposable derived content. Safe for any cleaner to delete.
    pub cache: PathBuf,
    pub config: PathBuf,
}

/// The directory name used under each root.
const QUALIFIER: &str = "rom-manager";

/// The reverse-DNS identifier macOS expects under Application Support.
const BUNDLE_ID: &str = "dev.mill1893.rom-manager";

impl AppPaths {
    /// Resolves paths from the environment, following the XDG base directory
    /// specification.
    ///
    /// `env` supplies variables so the resolution is testable without mutating
    /// the process environment, which would make tests order-dependent.
    pub fn resolve(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let home = env("HOME").map(PathBuf::from)?;

        let base = |variable: &str, fallback: &str| -> PathBuf {
            env(variable)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(fallback))
                .join(QUALIFIER)
        };

        Some(Self {
            data: base("XDG_DATA_HOME", ".local/share"),
            cache: base("XDG_CACHE_HOME", ".cache"),
            config: base("XDG_CONFIG_HOME", ".config"),
        })
    }

    /// Resolves macOS paths, which follow Apple's layout rather than XDG.
    ///
    /// The same guarantee holds, expressed differently: Application Support is
    /// backed up and survives, Caches is explicitly disposable and macOS will
    /// purge it under disk pressure without asking. Putting the Library in
    /// Caches would let the operating system itself delete the user's content.
    pub fn resolve_macos(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let home = env("HOME").map(PathBuf::from)?;
        Some(Self {
            data: home.join("Library/Application Support").join(BUNDLE_ID),
            cache: home.join("Library/Caches").join(BUNDLE_ID),
            config: home.join("Library/Preferences").join(BUNDLE_ID),
        })
    }

    /// Resolves from the real process environment, using the host's convention.
    pub fn from_env() -> Option<Self> {
        let read = |name: &str| std::env::var(name).ok();
        if cfg!(target_os = "macos") {
            Self::resolve_macos(read)
        } else {
            Self::resolve(read)
        }
    }

    /// App-owned Library storage.
    pub fn library_root(&self) -> PathBuf {
        self.data.join("library")
    }

    /// The durable database.
    pub fn database(&self) -> PathBuf {
        self.data.join("library.sqlite3")
    }

    /// The Materialization Cache. Under the cache root deliberately: a cleaner
    /// deleting it must cost time and never data.
    pub fn materialization_cache(&self) -> PathBuf {
        self.cache.join("materializations")
    }

    /// Whether `path` is somewhere the application must never write.
    ///
    /// An AppImage mounts itself read-only at a path that disappears on exit,
    /// so anything written there is lost the moment the user closes the app.
    pub fn is_ephemeral_mount(path: &std::path::Path) -> bool {
        let text = path.to_string_lossy();
        text.starts_with("/tmp/.mount_") || text.starts_with("/run/user/")
    }
}
