//! Coverage for the optional TheGamesDB adapter (issue #30).
//!
//! Every test runs with no network stack present — the adapter talks to a
//! transport, not a socket, which is what keeps the default build free of
//! network-capable code.

use std::collections::BTreeMap;

use rom_manager::{
    Allowance, BatchRefusal, CachedLookup, LookupOutcome, Provider, ProviderFailure,
    ProviderRecord, ProviderTransport, provider_artwork_may_reach_a_media_target, redact,
};

const DAY: i64 = 24 * 60 * 60;

/// A transport that answers from a script, counting what it was asked.
struct FakeTransport {
    allowance: Result<Allowance, ProviderFailure>,
    answers: Vec<Result<LookupOutcome, ProviderFailure>>,
    lookups: usize,
}

impl FakeTransport {
    fn with(answers: Vec<Result<LookupOutcome, ProviderFailure>>) -> Self {
        Self {
            allowance: Ok(Allowance { remaining: 100 }),
            answers,
            lookups: 0,
        }
    }
}

impl ProviderTransport for FakeTransport {
    fn allowance(&mut self) -> Result<Allowance, ProviderFailure> {
        self.allowance.clone()
    }

    fn lookup_by_hash(
        &mut self,
        _platform: &str,
        _sha256: &str,
    ) -> Result<LookupOutcome, ProviderFailure> {
        self.lookups += 1;
        self.answers
            .get(self.lookups - 1)
            .cloned()
            .unwrap_or(Ok(LookupOutcome::NotFound))
    }
}

fn record() -> ProviderRecord {
    ProviderRecord {
        provider_id: "tgdb-1234".into(),
        platform: "NES".into(),
        title: "Tracers".into(),
        fields: BTreeMap::new(),
        source_url: Some("https://thegamesdb.net/game.php?id=1234".into()),
        retrieved_at: 100,
        attribution: rom_manager::wire::thegamesdb_attribution(),
    }
}

#[test]
fn a_key_never_appears_in_anything_readable() {
    let key = "tgdb-secret-key-abcdef";
    let log = format!("GET /Games/ByGameID?apikey={key}&id=1");

    let redacted = redact(&log, key);
    assert!(!redacted.contains(key));
    assert!(redacted.contains("[REDACTED]"));
}

#[test]
fn provider_artwork_never_reaches_a_media_target() {
    // A device the user may hand to someone else. Copying provider artwork
    // there would redistribute content we have no right to redistribute.
    assert!(!provider_artwork_may_reach_a_media_target());
}

#[test]
fn a_batch_is_refused_before_any_request_when_allowance_is_short() {
    // Starting a batch that runs out halfway leaves a partly-enriched Library
    // and no clear idea which half.
    let mut transport = FakeTransport::with(vec![]);
    transport.allowance = Ok(Allowance { remaining: 3 });
    let mut provider = Provider::new(transport);

    assert_eq!(
        provider.preflight(10),
        Err(BatchRefusal::InsufficientAllowance {
            needed: 10,
            remaining: 3
        })
    );
}

#[test]
fn an_unreadable_allowance_is_never_spent() {
    let mut transport = FakeTransport::with(vec![]);
    transport.allowance = Err(ProviderFailure::Authentication);
    let mut provider = Provider::new(transport);

    assert!(provider.preflight(1).is_err());
}

#[test]
fn a_sufficient_allowance_permits_the_batch() {
    let mut provider = Provider::new(FakeTransport::with(vec![]));
    assert_eq!(provider.preflight(10).unwrap().remaining, 100);
}

#[test]
fn a_unique_match_is_returned_and_cached() {
    let mut provider = Provider::new(FakeTransport::with(vec![Ok(LookupOutcome::Matched(
        record(),
    ))]));

    let first = provider.lookup("NES", "abc", 100).unwrap();
    assert!(matches!(first, LookupOutcome::Matched(_)));

    // Second lookup costs no request.
    let second = provider.lookup("NES", "abc", 200).unwrap();
    assert_eq!(first, second);
    assert_eq!(provider.cached_entry_count(), 1);
}

#[test]
fn ambiguity_produces_suggestions_not_a_match() {
    let mut provider = Provider::new(FakeTransport::with(vec![Ok(LookupOutcome::Suggestions(
        vec![record(), record()],
    ))]));

    let outcome = provider.lookup("NES", "abc", 100).unwrap();
    assert!(matches!(outcome, LookupOutcome::Suggestions(candidates) if candidates.len() == 2));
}

#[test]
fn failures_are_distinct_and_never_cached() {
    // A transient outage or a mistyped key must not become a durable
    // "this game does not exist".
    for failure in [
        ProviderFailure::Authentication,
        ProviderFailure::QuotaExhausted {
            retry_after_seconds: Some(60),
        },
        ProviderFailure::Transient {
            retry_after_seconds: None,
        },
        ProviderFailure::MalformedResponse,
        ProviderFailure::UnsupportedRepresentation,
        ProviderFailure::Ambiguous { candidates: 3 },
    ] {
        assert!(!failure.is_cacheable(), "{failure:?} must never be cached");

        let mut provider = Provider::new(FakeTransport::with(vec![Err(failure.clone())]));
        assert_eq!(provider.lookup("NES", "abc", 100), Err(failure));
        assert_eq!(
            provider.cached_entry_count(),
            0,
            "a failed lookup leaves nothing behind"
        );
    }
}

#[test]
fn only_waiting_helps_for_quota_and_transient_failures() {
    assert!(
        ProviderFailure::Transient {
            retry_after_seconds: None
        }
        .is_retryable()
    );
    assert!(
        ProviderFailure::QuotaExhausted {
            retry_after_seconds: None
        }
        .is_retryable()
    );
    // These need the user, not a retry.
    assert!(!ProviderFailure::Authentication.is_retryable());
    assert!(!ProviderFailure::MalformedResponse.is_retryable());
}

#[test]
fn a_not_found_expires_after_a_day() {
    // A game absent today may be added tomorrow; a permanent not-found would be
    // wrong forever.
    let mut provider = Provider::new(FakeTransport::with(vec![
        Ok(LookupOutcome::NotFound),
        Ok(LookupOutcome::Matched(record())),
    ]));

    assert_eq!(
        provider.lookup("NES", "abc", 0).unwrap(),
        LookupOutcome::NotFound
    );
    assert!(provider.cached("NES", "abc", 0).is_some());

    // Still cached within the day.
    assert!(provider.cached("NES", "abc", DAY / 2).is_some());
    // Expired after it, so a fresh lookup happens.
    assert!(provider.cached("NES", "abc", DAY + 1).is_none());
    assert!(matches!(
        provider.lookup("NES", "abc", DAY + 1).unwrap(),
        LookupOutcome::Matched(_)
    ));
}

#[test]
fn a_stale_record_is_still_shown_offline() {
    // Offline usefulness beats freshness: 31-day-old cover art is better than
    // an empty pane.
    let mut provider = Provider::new(FakeTransport::with(vec![Ok(LookupOutcome::Matched(
        record(),
    ))]));
    provider.lookup("NES", "abc", 0).unwrap();

    let later = 31 * DAY;
    let cached = provider.cached("NES", "abc", later).expect("still shown");
    assert!(cached.is_stale(later), "and is marked stale");
}

#[test]
fn content_that_vanished_upstream_is_kept_and_labelled() {
    // Losing it would be worse than showing it with a caveat.
    let mut provider = Provider::new(FakeTransport::with(vec![Ok(LookupOutcome::Matched(
        record(),
    ))]));
    provider.lookup("NES", "abc", 100).unwrap();

    provider.mark_upstream_unavailable("NES", "abc");

    let cached = provider.cached("NES", "abc", 200).unwrap();
    assert!(cached.upstream_unavailable);
    assert!(matches!(cached.outcome, LookupOutcome::Matched(_)));
}

#[test]
fn clearing_removes_every_trace_of_provider_data() {
    let mut provider = Provider::new(FakeTransport::with(vec![
        Ok(LookupOutcome::Matched(record())),
        Ok(LookupOutcome::NotFound),
    ]));
    provider.lookup("NES", "abc", 100).unwrap();
    provider.lookup("NES", "def", 100).unwrap();
    assert_eq!(provider.cached_entry_count(), 2);

    provider.clear_provider_data();

    assert_eq!(provider.cached_entry_count(), 0);
    assert!(provider.cached("NES", "abc", 100).is_none());
}

#[test]
fn a_record_carries_its_provenance() {
    // Provider-controlled fields are labelled, linked, and timestamped, so a
    // user can tell them from their own facts.
    let record = record();

    assert!(record.source_url.is_some());
    assert_eq!(record.retrieved_at, 100);
    assert_eq!(record.platform, "NES");
}

#[test]
fn platform_scope_is_part_of_the_cache_identity() {
    // The same hash under a different Platform is a different question.
    let mut provider = Provider::new(FakeTransport::with(vec![
        Ok(LookupOutcome::Matched(record())),
        Ok(LookupOutcome::NotFound),
    ]));

    provider.lookup("NES", "abc", 100).unwrap();
    let other = provider.lookup("SNES", "abc", 100).unwrap();

    assert_eq!(other, LookupOutcome::NotFound);
    assert_eq!(provider.cached_entry_count(), 2);
}

// ── Attribution and terms provenance (issue #30, under #29 and #9) ──────────

#[test]
fn every_record_carries_the_credit_line_it_must_be_shown_with() {
    // Displaying provider data without saying whose it is is using someone
    // else's work uncredited, whatever the intent.
    let record = record();
    assert!(!record.attribution.notice.trim().is_empty());
    assert!(record.attribution.notice.contains("TheGamesDB"));
    assert!(record.attribution.terms_url.starts_with("https://"));
    assert!(record.may_be_displayed());
}

#[test]
fn a_record_from_terms_this_build_does_not_know_is_withheld() {
    // Fail-closed. The cost is a lookup the user can repeat; the alternative is
    // quietly pasting today's notice over data obtained under other terms.
    let mut record = record();
    record.attribution.terms_version = rom_manager::ACCEPTED_TERMS_VERSION + 1;

    assert!(
        !record.may_be_displayed(),
        "an unknown terms revision must withhold the record"
    );
}

#[test]
fn a_record_with_no_credit_line_is_never_displayable() {
    let mut record = record();
    record.attribution.notice = "   ".into();
    assert!(!record.may_be_displayed());
}

#[test]
fn a_cached_entry_predating_a_terms_change_stops_being_usable() {
    // Freshness and permission are different questions. An entry cached
    // yesterday under superseded terms is fresh and not usable.
    let mut stale_terms = record();
    stale_terms.attribution.terms_version = rom_manager::ACCEPTED_TERMS_VERSION + 1;

    let cached = CachedLookup {
        outcome: LookupOutcome::Matched(stale_terms),
        cached_at: 1_000,
        upstream_unavailable: false,
    };

    assert!(!cached.attribution_is_current());
    assert!(
        !cached.is_usable(1_100),
        "a fresh entry under unknown terms is still not usable"
    );
}

#[test]
fn one_suggestion_under_unknown_terms_withholds_the_whole_set() {
    // Showing three of four suggestions and silently dropping the fourth would
    // present an incomplete list as a complete one.
    let mut bad = record();
    bad.attribution.terms_version = rom_manager::ACCEPTED_TERMS_VERSION + 1;

    let cached = CachedLookup {
        outcome: LookupOutcome::Suggestions(vec![record(), bad]),
        cached_at: 1_000,
        upstream_unavailable: false,
    };
    assert!(!cached.attribution_is_current());
}

#[test]
fn a_negative_result_needs_no_attribution() {
    // There is nothing of the provider's to credit in "we found nothing".
    let cached = CachedLookup {
        outcome: LookupOutcome::NotFound,
        cached_at: 1_000,
        upstream_unavailable: false,
    };
    assert!(cached.attribution_is_current());
}

#[test]
fn clearing_provider_data_removes_the_attribution_with_it() {
    let mut provider = Provider::new(FakeTransport {
        allowance: Ok(Allowance { remaining: 10 }),
        answers: vec![Ok(LookupOutcome::Matched(record()))],
        lookups: 0,
    });
    provider.preflight(1).expect("allowance");
    provider.lookup("NES", "abc", 100).expect("a lookup");
    assert_eq!(provider.cached_entry_count(), 1);

    provider.clear_provider_data();
    assert_eq!(
        provider.cached_entry_count(),
        0,
        "clearing must leave no provider-derived text behind, credit included"
    );
}
