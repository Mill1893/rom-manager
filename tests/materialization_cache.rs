//! Coverage for the Materialization Cache (issue #60).
//!
//! The property under test throughout: **clearing the cache is always safe**.
//! Everything here is derived, so nothing it holds is the only copy of
//! anything.

use rom_manager::{CacheError, MaterializationCache, sha256};

fn cache(limit: u64) -> (tempfile::TempDir, MaterializationCache) {
    let directory = tempfile::tempdir().unwrap();
    let cache = MaterializationCache::open(directory.path().join("cache"), limit).unwrap();
    (directory, cache)
}

#[test]
fn entries_round_trip_by_content_identity() {
    let (_dir, cache) = cache(1 << 20);
    let bytes = b"materialized rom bytes".to_vec();
    let digest = sha256(&bytes);

    assert_eq!(cache.get(&digest).unwrap(), None, "cold cache is a miss");
    cache.put(&digest, &bytes).unwrap();
    assert_eq!(cache.get(&digest).unwrap(), Some(bytes));
}

#[test]
fn storing_bytes_under_the_wrong_digest_is_refused() {
    let (_dir, cache) = cache(1 << 20);
    assert!(matches!(
        cache.put(&"0".repeat(64), b"not those bytes"),
        Err(CacheError::Corrupt)
    ));
}

#[test]
fn a_corrupted_entry_reads_as_a_miss_and_is_discarded() {
    // A cache that hands back the wrong bytes is worse than no cache.
    let (dir, cache) = cache(1 << 20);
    let bytes = b"materialized rom bytes".to_vec();
    let digest = sha256(&bytes);
    cache.put(&digest, &bytes).unwrap();

    std::fs::write(dir.path().join("cache").join(&digest), b"tampered").unwrap();

    assert_eq!(
        cache.get(&digest).unwrap(),
        None,
        "a mismatched entry must be a miss, never a wrong answer"
    );
    assert_eq!(cache.entry_count().unwrap(), 0, "and must be discarded");
}

#[test]
fn the_cache_evicts_least_recently_used_entries_to_stay_within_its_limit() {
    let (_dir, cache) = cache(30);

    for index in 0..4u8 {
        let bytes = vec![index; 10];
        cache.put(&sha256(&bytes), &bytes).unwrap();
        // Distinct modification times so LRU ordering is well defined.
        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    assert!(
        cache.size_bytes().unwrap() <= 30,
        "the cache must respect its configured limit"
    );
    // The most recent entry survives.
    let newest = vec![3u8; 10];
    assert!(cache.get(&sha256(&newest)).unwrap().is_some());
}

#[test]
fn a_leased_entry_is_never_evicted() {
    // A long-running sync must not have its working set deleted by an
    // unrelated import filling the cache.
    let (_dir, cache) = cache(20);
    let held = vec![9u8; 10];
    let held_digest = sha256(&held);
    cache.put(&held_digest, &held).unwrap();

    let lease = cache.lease(&held_digest);
    std::thread::sleep(std::time::Duration::from_millis(15));

    for index in 0..4u8 {
        let bytes = vec![index; 10];
        cache.put(&sha256(&bytes), &bytes).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(
        cache.get(&held_digest).unwrap().is_some(),
        "the leased entry survived eviction pressure"
    );
    drop(lease);

    // Once released it is an ordinary candidate again.
    for index in 4..8u8 {
        let bytes = vec![index; 10];
        cache.put(&sha256(&bytes), &bytes).unwrap();
    }
    assert!(cache.size_bytes().unwrap() <= 20);
}

#[test]
fn clearing_is_safe_and_leaves_nothing_behind() {
    let (_dir, cache) = cache(1 << 20);
    for index in 0..5u8 {
        let bytes = vec![index; 32];
        cache.put(&sha256(&bytes), &bytes).unwrap();
    }
    assert_eq!(cache.entry_count().unwrap(), 5);

    cache.clear().unwrap();
    assert_eq!(cache.entry_count().unwrap(), 0);

    // And the cache is still perfectly usable afterwards.
    let bytes = b"fresh".to_vec();
    let digest = sha256(&bytes);
    cache.put(&digest, &bytes).unwrap();
    assert_eq!(cache.get(&digest).unwrap(), Some(bytes));
}

#[test]
fn clearing_keeps_entries_that_are_in_use() {
    let (_dir, cache) = cache(1 << 20);
    let bytes = b"in use".to_vec();
    let digest = sha256(&bytes);
    cache.put(&digest, &bytes).unwrap();

    let _lease = cache.lease(&digest);
    cache.clear().unwrap();

    assert!(
        cache.get(&digest).unwrap().is_some(),
        "clearing must not break an operation that is mid-flight"
    );
}

#[test]
fn a_partially_written_entry_is_never_observed() {
    // Entries are renamed into place, so staging files are not visible as
    // cache entries even before they land.
    let (dir, cache) = cache(1 << 20);
    std::fs::write(dir.path().join("cache").join(".staging-abc"), b"partial").unwrap();

    assert_eq!(
        cache.entry_count().unwrap(),
        0,
        "a staging file is not a cache entry"
    );
}
