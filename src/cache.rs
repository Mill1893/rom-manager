//! The Materialization Cache (issue #60).
//!
//! # Disposable by construction
//!
//! Everything here can be deleted at any moment without losing anything. The
//! cache holds *derived* bytes — materializations reproduced from content the
//! Library already owns — so clearing it costs time, never data.
//!
//! That is why availability is never established by the cache: a ROM is
//! available when a healthy managed Source Occurrence can reproduce it, and a
//! cached copy is evidence of nothing. If the cache could make something
//! available, clearing it could make something unavailable, and clearing would
//! stop being safe.
//!
//! # Verified, not trusted
//!
//! An entry is keyed by the content digest it claims to hold, and every read
//! re-verifies before returning. A cache that hands back the wrong bytes
//! because a file was corrupted underneath it would be worse than no cache, so
//! a mismatched entry is discarded and treated as a miss.
//!
//! # Leases
//!
//! An entry being *used* cannot be evicted. Eviction picks the least recently
//! used entry, but skips anything leased — otherwise a long-running sync could
//! have the file deleted out from under it by an unrelated import filling the
//! cache.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::sha256;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("the cache could not be read or written: {0}")]
    Io(String),
    #[error("cached bytes did not match the digest they were filed under")]
    Corrupt,
}

/// A held entry. While one exists, the entry it names cannot be evicted.
pub struct Lease {
    digest: String,
    leases: Arc<Mutex<HashMap<String, usize>>>,
}

impl Lease {
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut leases) = self.leases.lock()
            && let Some(count) = leases.get_mut(&self.digest)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                leases.remove(&self.digest);
            }
        }
    }
}

pub struct MaterializationCache {
    root: PathBuf,
    /// Bytes the cache may hold before eviction runs. User-configurable.
    limit_bytes: u64,
    leases: Arc<Mutex<HashMap<String, usize>>>,
}

impl MaterializationCache {
    pub fn open(root: impl Into<PathBuf>, limit_bytes: u64) -> Result<Self, CacheError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|error| CacheError::Io(error.to_string()))?;
        Ok(Self {
            root,
            limit_bytes,
            leases: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn entry_path(&self, digest: &str) -> PathBuf {
        self.root.join(digest)
    }

    /// Returns cached bytes for `digest`, or `None` on a miss.
    ///
    /// Re-verifies before returning. An entry whose bytes no longer match the
    /// digest it is filed under is discarded and reported as a miss rather than
    /// handed back.
    pub fn get(&self, digest: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let path = self.entry_path(digest);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|error| CacheError::Io(error.to_string()))?;
        if sha256(&bytes) != digest {
            let _ = fs::remove_file(&path);
            return Ok(None);
        }
        // Touch so least-recently-used ordering reflects real use.
        let _ = filetime_now(&path);
        Ok(Some(bytes))
    }

    /// Stores `bytes` under their own digest, atomically.
    ///
    /// Written to a temporary name and renamed into place, so a reader never
    /// observes a partially written entry. Storing bytes whose digest does not
    /// match what they claim is refused.
    pub fn put(&self, digest: &str, bytes: &[u8]) -> Result<(), CacheError> {
        if sha256(bytes) != digest {
            return Err(CacheError::Corrupt);
        }
        let staged = self.root.join(format!(".staging-{digest}"));
        fs::write(&staged, bytes).map_err(|error| CacheError::Io(error.to_string()))?;
        fs::rename(&staged, self.entry_path(digest))
            .map_err(|error| CacheError::Io(error.to_string()))?;
        self.evict_to_limit()
    }

    /// Takes a lease on an entry, protecting it from eviction while held.
    pub fn lease(&self, digest: &str) -> Lease {
        if let Ok(mut leases) = self.leases.lock() {
            *leases.entry(digest.to_owned()).or_insert(0) += 1;
        }
        Lease {
            digest: digest.to_owned(),
            leases: Arc::clone(&self.leases),
        }
    }

    fn is_leased(&self, digest: &str) -> bool {
        self.leases
            .lock()
            .map(|leases| leases.contains_key(digest))
            .unwrap_or(false)
    }

    pub fn size_bytes(&self) -> Result<u64, CacheError> {
        Ok(self.entries()?.iter().map(|entry| entry.1).sum())
    }

    pub fn entry_count(&self) -> Result<usize, CacheError> {
        Ok(self.entries()?.len())
    }

    /// `(digest, size, last_used)` for every entry.
    fn entries(&self) -> Result<Vec<(String, u64, std::time::SystemTime)>, CacheError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|error| CacheError::Io(error.to_string()))? {
            let entry = entry.map_err(|error| CacheError::Io(error.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".staging-") {
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|error| CacheError::Io(error.to_string()))?;
            let used = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            entries.push((name, metadata.len(), used));
        }
        Ok(entries)
    }

    /// Evicts least-recently-used entries until the cache fits its limit.
    ///
    /// Leased entries are skipped, so a sync in flight never has its working
    /// set deleted by an unrelated import.
    pub fn evict_to_limit(&self) -> Result<(), CacheError> {
        let mut entries = self.entries()?;
        let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
        if total <= self.limit_bytes {
            return Ok(());
        }
        entries.sort_by_key(|entry| entry.2);

        for (digest, size, _) in entries {
            if total <= self.limit_bytes {
                break;
            }
            if self.is_leased(&digest) {
                continue;
            }
            if fs::remove_file(self.entry_path(&digest)).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }

    /// Empties the cache. Always safe: nothing here is the only copy of
    /// anything, so this can never make content unavailable.
    ///
    /// Leased entries are kept — they are in use, and removing them would break
    /// an operation rather than free space that matters.
    pub fn clear(&self) -> Result<(), CacheError> {
        for (digest, _, _) in self.entries()? {
            if self.is_leased(&digest) {
                continue;
            }
            let _ = fs::remove_file(self.entry_path(&digest));
        }
        Ok(())
    }
}

fn filetime_now(path: &Path) -> std::io::Result<()> {
    // Rewriting the file's own bytes would be wasteful; opening for append and
    // syncing is enough to update the modification time on every supported
    // platform.
    let file = fs::OpenOptions::new().append(true).open(path)?;
    file.set_modified(std::time::SystemTime::now())
}
