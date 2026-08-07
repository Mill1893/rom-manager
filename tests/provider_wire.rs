//! Coverage for TheGamesDB response handling, against checked-in fixtures
//! (issue #30).
//!
//! Every fixture is hand-authored from the public schema. No response from
//! TheGamesDB is committed here — #29 forbids bundling provider content, and a
//! recorded response would breach that permanently.

use rom_manager::{FixtureTransport, LookupOutcome, Provider, ProviderFailure, wire};

const ALLOWANCE: &str = include_str!("../fixtures/thegamesdb/allowance.json");
const UNIQUE: &str = include_str!("../fixtures/thegamesdb/match-unique.json");
const AMBIGUOUS: &str = include_str!("../fixtures/thegamesdb/match-ambiguous.json");
const NOT_FOUND: &str = include_str!("../fixtures/thegamesdb/not-found.json");
const AUTH_ERROR: &str = include_str!("../fixtures/thegamesdb/error-auth.json");
const QUOTA_ERROR: &str = include_str!("../fixtures/thegamesdb/error-quota.json");
const MALFORMED: &str = include_str!("../fixtures/thegamesdb/malformed.json");

#[test]
fn the_allowance_endpoint_is_read() {
    assert_eq!(wire::parse_allowance(ALLOWANCE).unwrap().remaining, 2996);
}

#[test]
fn a_single_platform_consistent_result_may_match() {
    let outcome = wire::parse_lookup(UNIQUE, "NES", 100).unwrap();

    let LookupOutcome::Matched(record) = outcome else {
        panic!("a unique result should match");
    };
    assert_eq!(record.provider_id, "tgdb-100001");
    assert_eq!(record.platform, "NES");
    assert_eq!(record.title, "Placeholder Game One");
    assert_eq!(record.fields["releasedate"], "1994-03-17");
    assert_eq!(record.fields["players"], "2");
    assert!(record.source_url.unwrap().contains("100001"));
}

#[test]
fn several_candidates_become_suggestions_never_a_match() {
    // An auto-match the user did not review is a silent claim about their
    // Library.
    let outcome = wire::parse_lookup(AMBIGUOUS, "NES", 100).unwrap();

    let LookupOutcome::Suggestions(records) = outcome else {
        panic!("ambiguity must not auto-match");
    };
    assert_eq!(records.len(), 2);
}

#[test]
fn an_empty_result_is_a_definitive_not_found() {
    assert_eq!(
        wire::parse_lookup(NOT_FOUND, "NES", 100).unwrap(),
        LookupOutcome::NotFound
    );
}

#[test]
fn a_rejected_key_is_an_authentication_failure() {
    assert_eq!(
        wire::parse_lookup(AUTH_ERROR, "NES", 100),
        Err(ProviderFailure::Authentication)
    );
}

#[test]
fn exhausted_quota_carries_its_retry_hint() {
    assert_eq!(
        wire::parse_lookup(QUOTA_ERROR, "NES", 100),
        Err(ProviderFailure::QuotaExhausted {
            retry_after_seconds: Some(3600)
        })
    );
}

#[test]
fn an_unreadable_shape_is_malformed_not_empty() {
    // The decisive distinction. `games` present but not an array means the
    // adapter cannot read the response — treating it as "no results" would
    // cache a wrong answer for a day.
    assert_eq!(
        wire::parse_lookup(MALFORMED, "NES", 100),
        Err(ProviderFailure::MalformedResponse)
    );
}

#[test]
fn nonsense_is_rejected_rather_than_guessed_at() {
    for body in ["", "not json at all", "{}", "[]"] {
        assert!(
            wire::parse_lookup(body, "NES", 100).is_err(),
            "{body:?} must not parse"
        );
    }
}

#[test]
fn the_adapter_runs_end_to_end_against_fixtures() {
    let transport =
        FixtureTransport::new(ALLOWANCE, vec![UNIQUE.to_string(), NOT_FOUND.to_string()]);
    let mut provider = Provider::new(transport);

    assert_eq!(provider.preflight(2).unwrap().remaining, 2996);

    assert!(matches!(
        provider.lookup("NES", "aaa", 100).unwrap(),
        LookupOutcome::Matched(_)
    ));
    assert_eq!(
        provider.lookup("NES", "bbb", 100).unwrap(),
        LookupOutcome::NotFound
    );
    assert_eq!(provider.cached_entry_count(), 2);
}

#[test]
fn a_malformed_response_leaves_nothing_cached() {
    let transport = FixtureTransport::new(ALLOWANCE, vec![MALFORMED.to_string()]);
    let mut provider = Provider::new(transport);

    assert!(provider.lookup("NES", "aaa", 100).is_err());
    assert_eq!(
        provider.cached_entry_count(),
        0,
        "an unreadable response must not become a cached answer"
    );
}

#[test]
fn no_fixture_contains_recorded_provider_content() {
    // A guard against someone helpfully pasting in a real response later.
    for fixture in [UNIQUE, AMBIGUOUS, NOT_FOUND] {
        assert!(
            fixture.contains("Placeholder") || fixture.contains("\"games\": []"),
            "fixtures must use invented content, never recorded responses"
        );
    }
}

#[test]
fn attribution_is_stamped_when_a_record_is_parsed_not_when_it_is_shown() {
    // Configuration describes the provider *now*; a cached record may have
    // arrived a year ago under different terms. Re-labelling old data with
    // today's notice is a guess, not provenance — so the credit line and terms
    // are fixed at the moment the bytes are read.
    let outcome = wire::parse_lookup(UNIQUE, "NES", 500).unwrap();
    let LookupOutcome::Matched(record) = outcome else {
        panic!("the unique fixture matches");
    };

    assert_eq!(record.retrieved_at, 500);
    assert_eq!(record.attribution, wire::thegamesdb_attribution());
    assert!(record.attribution.notice.contains("TheGamesDB"));
    assert!(record.may_be_displayed());
}

#[test]
fn every_parsed_suggestion_carries_attribution_too() {
    // A suggestion list is provider data as much as a match is.
    let outcome = wire::parse_lookup(AMBIGUOUS, "NES", 100).unwrap();
    let LookupOutcome::Suggestions(records) = outcome else {
        panic!("the ambiguous fixture suggests");
    };
    assert!(records.len() > 1);
    for record in records {
        assert!(record.may_be_displayed(), "{}", record.title);
    }
}

#[test]
fn no_fixture_here_is_a_recorded_response_from_the_provider() {
    // #29 forbids bundling provider content, and a recorded response would
    // breach that permanently. Every fixture is hand-authored from the schema,
    // so none should carry a real game's description.
    for fixture in [UNIQUE, AMBIGUOUS, NOT_FOUND, ALLOWANCE] {
        assert!(fixture.len() < 8 * 1024, "a fixture grew to response size");
    }
}
