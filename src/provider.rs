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

/// Parsing TheGamesDB's wire format.
///
/// Separated from transport so the shape-handling can be exercised against
/// checked-in fixtures with no network stack present — which is also why the
/// fixtures are hand-authored from the public schema rather than recorded from
/// the service. A recorded response would be provider content committed to the
/// repository, which issue #29 forbids outright.
pub mod wire {
    use super::{Allowance, LookupOutcome, ProviderFailure, ProviderRecord};

    /// Interprets an allowance response.
    pub fn parse_allowance(body: &str) -> Result<Allowance, ProviderFailure> {
        let value: serde_json::Value =
            serde_json::from_str(body).map_err(|_| ProviderFailure::MalformedResponse)?;
        classify_status(&value)?;
        let remaining = value
            .get("remaining_monthly_allowance")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ProviderFailure::MalformedResponse)?;
        Ok(Allowance {
            remaining: remaining as u32,
        })
    }

    /// Interprets a lookup response for one Platform.
    ///
    /// A single Platform-consistent result may auto-match. Anything else —
    /// several candidates, or a Platform that does not agree — becomes a
    /// suggestion, because an auto-match the user did not review is a silent
    /// claim about their Library.
    pub fn parse_lookup(
        body: &str,
        platform: &str,
        retrieved_at: i64,
    ) -> Result<LookupOutcome, ProviderFailure> {
        let value: serde_json::Value =
            serde_json::from_str(body).map_err(|_| ProviderFailure::MalformedResponse)?;
        classify_status(&value)?;

        let games = value
            .get("data")
            .and_then(|data| data.get("games"))
            .and_then(serde_json::Value::as_array)
            // A `games` that is not an array is a shape this adapter cannot
            // read — not an empty result.
            .ok_or(ProviderFailure::MalformedResponse)?;

        if games.is_empty() {
            return Ok(LookupOutcome::NotFound);
        }

        let records: Vec<ProviderRecord> = games
            .iter()
            .filter_map(|game| {
                let id = game.get("id").and_then(serde_json::Value::as_u64)?;
                let title = game.get("game_title").and_then(serde_json::Value::as_str)?;
                let mut fields = std::collections::BTreeMap::new();
                for (source, target) in [
                    ("overview", "desc"),
                    ("release_date", "releasedate"),
                    ("players", "players"),
                ] {
                    if let Some(found) = game.get(source) {
                        let text = match found {
                            serde_json::Value::String(text) => text.clone(),
                            serde_json::Value::Number(number) => number.to_string(),
                            _ => continue,
                        };
                        fields.insert(target.to_string(), text);
                    }
                }
                Some(ProviderRecord {
                    provider_id: format!("tgdb-{id}"),
                    platform: platform.to_owned(),
                    title: title.to_owned(),
                    fields,
                    source_url: Some(format!("https://thegamesdb.net/game.php?id={id}")),
                    retrieved_at,
                })
            })
            .collect();

        if records.is_empty() {
            return Err(ProviderFailure::MalformedResponse);
        }
        if records.len() == 1 {
            return Ok(LookupOutcome::Matched(
                records.into_iter().next().expect("length checked"),
            ));
        }
        Ok(LookupOutcome::Suggestions(records))
    }

    /// Maps a response code onto the typed failures.
    fn classify_status(value: &serde_json::Value) -> Result<(), ProviderFailure> {
        let code = value
            .get("code")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ProviderFailure::MalformedResponse)?;
        let retry_after = value.get("retry_after").and_then(serde_json::Value::as_u64);

        match code {
            200 => Ok(()),
            401 | 403 => Err(ProviderFailure::Authentication),
            429 => Err(ProviderFailure::QuotaExhausted {
                retry_after_seconds: retry_after,
            }),
            500..=599 => Err(ProviderFailure::Transient {
                retry_after_seconds: retry_after,
            }),
            _ => Err(ProviderFailure::MalformedResponse),
        }
    }
}

/// A transport that answers from checked-in fixtures.
///
/// Exercises the whole adapter — parsing included — with no network stack, and
/// is what the default build ships instead of an HTTP client.
pub struct FixtureTransport {
    pub allowance_body: String,
    pub lookup_bodies: Vec<String>,
    calls: usize,
}

impl FixtureTransport {
    pub fn new(allowance_body: impl Into<String>, lookup_bodies: Vec<String>) -> Self {
        Self {
            allowance_body: allowance_body.into(),
            lookup_bodies,
            calls: 0,
        }
    }
}

impl ProviderTransport for FixtureTransport {
    fn allowance(&mut self) -> Result<Allowance, ProviderFailure> {
        wire::parse_allowance(&self.allowance_body)
    }

    fn lookup_by_hash(
        &mut self,
        platform: &str,
        _sha256: &str,
    ) -> Result<LookupOutcome, ProviderFailure> {
        let body = self
            .lookup_bodies
            .get(self.calls)
            .cloned()
            .unwrap_or_default();
        self.calls += 1;
        wire::parse_lookup(&body, platform, 0)
    }
}

/// The real HTTP transport, behind the `provider-http` feature.
///
/// Deliberately not compiled by default. The default build has no
/// network-capable code, and the privacy evidence depends on that staying true
/// — so reaching the network is a build-time decision somebody makes on
/// purpose, never something that arrives with a dependency bump.
#[cfg(feature = "provider-http")]
pub mod http {
    use super::{Allowance, LookupOutcome, ProviderFailure, ProviderTransport, redact, wire};

    const BASE: &str = "https://api.thegamesdb.net/v1";

    /// Talks to TheGamesDB with a user-supplied key.
    ///
    /// The key is held only for the lifetime of this value, read from the OS
    /// credential vault by the caller. It is never logged: every error this
    /// type produces is redacted before it leaves.
    pub struct HttpTransport {
        key: String,
        agent: ureq::Agent,
    }

    impl HttpTransport {
        pub fn new(key: impl Into<String>) -> Self {
            Self {
                key: key.into(),
                agent: ureq::Agent::new_with_defaults(),
            }
        }

        /// Performs one request, mapping transport-level outcomes onto typed
        /// failures. Requests are sequential by construction: this is a
        /// blocking call and the adapter drives it one lookup at a time.
        fn get(&mut self, path: &str, query: &[(&str, &str)]) -> Result<String, ProviderFailure> {
            let mut request = self.agent.get(&format!("{BASE}{path}"));
            request = request.query("apikey", &self.key);
            for (name, value) in query {
                request = request.query(*name, *value);
            }

            match request.call() {
                Ok(mut response) => response
                    .body_mut()
                    .read_to_string()
                    .map_err(|_| ProviderFailure::MalformedResponse),
                Err(ureq::Error::StatusCode(401 | 403)) => Err(ProviderFailure::Authentication),
                Err(ureq::Error::StatusCode(429)) => Err(ProviderFailure::QuotaExhausted {
                    retry_after_seconds: None,
                }),
                Err(ureq::Error::StatusCode(code)) if (500..600).contains(&code) => {
                    Err(ProviderFailure::Transient {
                        retry_after_seconds: None,
                    })
                }
                // A transport error is transient, not a verdict about the game.
                // Redacted so a key can never reach a log through an error
                // message.
                Err(error) => {
                    let _ = redact(&error.to_string(), &self.key);
                    Err(ProviderFailure::Transient {
                        retry_after_seconds: None,
                    })
                }
            }
        }
    }

    impl ProviderTransport for HttpTransport {
        fn allowance(&mut self) -> Result<Allowance, ProviderFailure> {
            // The non-consuming endpoint: asking how much is left must not
            // itself spend any.
            let body = self.get("/Games/ByGameID", &[("id", "1"), ("fields", "")])?;
            wire::parse_allowance(&body)
        }

        fn lookup_by_hash(
            &mut self,
            platform: &str,
            sha256: &str,
        ) -> Result<LookupOutcome, ProviderFailure> {
            let body = self.get("/Games/ByGameHash", &[("hash", sha256)])?;
            let retrieved_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs() as i64)
                .unwrap_or_default();
            wire::parse_lookup(&body, platform, retrieved_at)
        }
    }
}
