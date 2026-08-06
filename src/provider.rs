//! The optional TheGamesDB provider adapter (issue #30).
//!
//! # Optional means optional
//!
//! Every import, correction, Library, ROM Pack, and sync workflow is complete
//! without a key, without network, without quota, and without the provider
//! being up. Provider lookup is an enhancement layered on top, and nothing
//! below it may come to depend on it.
//!
//! # No network in the default build
//!
//! This module talks to a [`ProviderTransport`], not to a socket. The only
//! implementation shipped by default is the offline fake used by tests, so the
//! default build still contains no network-capable code at all — the property
//! the privacy evidence rests on. A real HTTP implementation lives behind an
//! opt-in feature, so enabling network access is a deliberate build-time act
//! rather than something that arrives silently with a dependency bump.
//!
//! # The key is never ours
//!
//! Each user supplies their own key. No shared key ships, no proxy is operated,
//! and the key lives in the operating-system credential vault — the Library
//! holds only a non-secret reference to it. Everything that could be read by
//! someone else — logs, diagnostics, exports, fixtures — redacts it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A non-secret handle to a credential held in the OS vault.
///
/// This is what the Library stores. It names where the key is, never what it
/// is, so a leaked database leaks nothing usable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CredentialReference {
    pub vault_entry: String,
}

/// Redacts anything that looks like a credential.
///
/// Applied at every boundary a human or another program can read. A key that
/// reaches a log is as compromised as one that reaches a public repository.
pub fn redact(text: &str, key: &str) -> String {
    if key.is_empty() {
        return text.to_owned();
    }
    text.replace(key, "[REDACTED]")
}

/// Why a provider call did not produce a usable answer.
///
/// These are kept distinct because they demand different responses: a bad key
/// needs the user, exhausted quota needs waiting, a transient fault needs a
/// retry, and a definitive not-found needs neither.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderFailure {
    #[error("the API key was rejected")]
    Authentication,
    #[error("the request allowance is exhausted")]
    QuotaExhausted { retry_after_seconds: Option<u64> },
    #[error("the provider is temporarily unavailable")]
    Transient { retry_after_seconds: Option<u64> },
    #[error("the provider returned something this adapter cannot read")]
    MalformedResponse,
    #[error("this representation cannot be looked up by hash")]
    UnsupportedRepresentation,
    #[error("more than one Release matches")]
    Ambiguous { candidates: usize },
}

impl ProviderFailure {
    /// Whether retrying could plausibly succeed without user action.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Transient { .. } | Self::QuotaExhausted { .. })
    }

    /// Whether this may be cached as a lookup result.
    ///
    /// Only a definitive not-found may be. Caching a failure would turn a
    /// temporary outage or a mistyped key into a durable "this game does not
    /// exist".
    pub fn is_cacheable(&self) -> bool {
        false
    }
}

/// What the provider said.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupOutcome {
    /// Exact, unique, Platform-consistent evidence.
    Matched(ProviderRecord),
    /// Evidence exists but is not conclusive; the user decides.
    Suggestions(Vec<ProviderRecord>),
    /// The provider is confident there is nothing.
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub provider_id: String,
    pub platform: String,
    pub title: String,
    pub fields: BTreeMap<String, String>,
    /// Where this came from, shown next to anything it contributes.
    pub source_url: Option<String>,
    pub retrieved_at: i64,
}

/// The remaining allowance, as the provider reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Allowance {
    pub remaining: u32,
}

/// What the adapter needs from the outside world.
///
/// Deliberately narrow, and deliberately not HTTP: the adapter's logic can then
/// be exercised in full with no network stack present.
pub trait ProviderTransport {
    /// The non-consuming allowance endpoint, queried before every batch.
    fn allowance(&mut self) -> Result<Allowance, ProviderFailure>;
    fn lookup_by_hash(
        &mut self,
        platform: &str,
        sha256: &str,
    ) -> Result<LookupOutcome, ProviderFailure>;
}

/// A cached lookup result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedLookup {
    pub outcome: LookupOutcome,
    pub cached_at: i64,
    /// True once the provider record is gone upstream. The copy is kept and
    /// labelled rather than deleted — losing it would be worse than showing it
    /// with a caveat.
    pub upstream_unavailable: bool,
}

const STALE_AFTER_SECONDS: i64 = 30 * 24 * 60 * 60;
const NEGATIVE_CACHE_SECONDS: i64 = 24 * 60 * 60;

impl CachedLookup {
    /// Stale entries are still displayed — offline usefulness beats
    /// freshness — and are revalidated only when the user asks.
    pub fn is_stale(&self, now: i64) -> bool {
        now - self.cached_at > STALE_AFTER_SECONDS
    }

    /// A negative result expires much sooner: a game absent today may be added
    /// tomorrow, and a permanent "not found" would be wrong forever.
    pub fn negative_result_expired(&self, now: i64) -> bool {
        self.outcome == LookupOutcome::NotFound && now - self.cached_at > NEGATIVE_CACHE_SECONDS
    }

    pub fn is_usable(&self, now: i64) -> bool {
        !self.negative_result_expired(now)
    }
}

/// The provider adapter.
pub struct Provider<T: ProviderTransport> {
    transport: T,
    cache: BTreeMap<String, CachedLookup>,
}

/// Why a batch was refused before any request was made.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BatchRefusal {
    #[error("this lookup needs {needed} requests but only {remaining} remain")]
    InsufficientAllowance { needed: u32, remaining: u32 },
}

impl<T: ProviderTransport> Provider<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            cache: BTreeMap::new(),
        }
    }

    fn cache_key(platform: &str, sha256: &str) -> String {
        format!("{platform}/{sha256}")
    }

    /// Reads cache only. Never contacts the provider, so it is safe to call
    /// while rendering.
    pub fn cached(&self, platform: &str, sha256: &str, now: i64) -> Option<&CachedLookup> {
        self.cache
            .get(&Self::cache_key(platform, sha256))
            .filter(|entry| entry.is_usable(now))
    }

    /// Checks the allowance before a batch and refuses rather than starting
    /// work it cannot finish.
    ///
    /// Beginning a batch that will run out halfway leaves the user with a
    /// partly-enriched Library and no clear idea which half.
    pub fn preflight(&mut self, needed: u32) -> Result<Allowance, BatchRefusal> {
        let allowance = match self.transport.allowance() {
            Ok(allowance) => allowance,
            // An allowance we cannot read is not an allowance we may spend.
            Err(_) => {
                return Err(BatchRefusal::InsufficientAllowance {
                    needed,
                    remaining: 0,
                });
            }
        };
        if allowance.remaining < needed {
            return Err(BatchRefusal::InsufficientAllowance {
                needed,
                remaining: allowance.remaining,
            });
        }
        Ok(allowance)
    }

    /// One explicit lookup. Only ever called from a user action.
    pub fn lookup(
        &mut self,
        platform: &str,
        sha256: &str,
        now: i64,
    ) -> Result<LookupOutcome, ProviderFailure> {
        let key = Self::cache_key(platform, sha256);

        // Deduplicated: a cached usable answer costs no request.
        if let Some(entry) = self.cache.get(&key).filter(|entry| entry.is_usable(now)) {
            return Ok(entry.outcome.clone());
        }

        let outcome = self.transport.lookup_by_hash(platform, sha256)?;

        // Only successes and definitive not-founds are cached; failures never
        // reach here, so a transient outage cannot become a durable answer.
        self.cache.insert(
            key,
            CachedLookup {
                outcome: outcome.clone(),
                cached_at: now,
                upstream_unavailable: false,
            },
        );
        Ok(outcome)
    }

    /// Marks a cached record as gone upstream, keeping the copy.
    pub fn mark_upstream_unavailable(&mut self, platform: &str, sha256: &str) {
        if let Some(entry) = self.cache.get_mut(&Self::cache_key(platform, sha256)) {
            entry.upstream_unavailable = true;
        }
    }

    /// Removes every trace of provider data.
    ///
    /// Local identity, ROM content, Local Overrides, and user-supplied artwork
    /// are untouched — they were never the provider's to begin with.
    pub fn clear_provider_data(&mut self) {
        self.cache.clear();
    }

    pub fn cached_entry_count(&self) -> usize {
        self.cache.len()
    }
}

/// Whether provider-supplied artwork may be written to a Media Target.
///
/// Always false. Provider artwork is licensed for private in-app display, and a
/// Media Target is a device the user may hand to someone else — copying it
/// there would redistribute content ROM Manager has no right to redistribute.
pub const fn provider_artwork_may_reach_a_media_target() -> bool {
    false
}
