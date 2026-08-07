//! Coverage for Metadata Projections (issue #67).
//!
//! The theme: an absent field is honestly absent, while a wrong one is
//! indistinguishable from a right one. Anything uncertain is omitted.

use rom_manager::{
    CalendarDate, EntryEligibility, MetadataProjection, PlayerCount, ReleaseFacts,
    disambiguate_titles,
};

fn facts() -> ReleaseFacts {
    ReleaseFacts {
        title: "Tracers".into(),
        ..Default::default()
    }
}

fn project(facts: &ReleaseFacts) -> MetadataProjection {
    MetadataProjection::build("./Tracers.nes".into(), facts, &facts.title)
}

#[test]
fn mapped_fields_are_exported() {
    let projection = project(&ReleaseFacts {
        sort_title: Some("Tracers".into()),
        description: Some("A game.".into()),
        release_date: Some(CalendarDate::Complete {
            year: 1994,
            month: 3,
            day: 17,
        }),
        developers: vec!["Studio A".into()],
        publishers: vec!["Publisher B".into()],
        primary_genre: Some("Puzzle".into()),
        players: Some(PlayerCount::Exact(2)),
        ..facts()
    });

    assert_eq!(projection.fields["name"], "Tracers");
    assert_eq!(projection.fields["sortname"], "Tracers");
    assert_eq!(projection.fields["desc"], "A game.");
    assert_eq!(projection.fields["releasedate"], "19940317T000000");
    assert_eq!(projection.fields["developer"], "Studio A");
    assert_eq!(projection.fields["publisher"], "Publisher B");
    assert_eq!(projection.fields["genre"], "Puzzle");
    assert_eq!(projection.fields["players"], "2");
}

#[test]
fn a_partial_date_is_omitted_rather_than_invented() {
    // Rendering a year-only date as 1994-01-01 would read as fact.
    let projection = project(&ReleaseFacts {
        release_date: Some(CalendarDate::Partial),
        ..facts()
    });

    assert!(!projection.fields.contains_key("releasedate"));
}

#[test]
fn an_open_ended_player_count_is_omitted() {
    // "2 or more" rendered as "2" would be a lie the user cannot detect.
    let open = project(&ReleaseFacts {
        players: Some(PlayerCount::Open),
        ..facts()
    });
    assert!(!open.fields.contains_key("players"));

    let closed = project(&ReleaseFacts {
        players: Some(PlayerCount::Range { min: 1, max: 4 }),
        ..facts()
    });
    assert_eq!(closed.fields["players"], "1-4");
}

#[test]
fn multiple_credits_join_deterministically() {
    let projection = project(&ReleaseFacts {
        developers: vec!["Studio A".into(), "Studio B".into()],
        publishers: vec!["Pub One".into(), "Pub Two".into(), "Pub Three".into()],
        ..facts()
    });

    assert_eq!(projection.fields["developer"], "Studio A / Studio B");
    assert_eq!(
        projection.fields["publisher"],
        "Pub One / Pub Two / Pub Three"
    );
}

#[test]
fn nothing_unmappable_is_exported() {
    // Everything present, and the owned set is still exactly the mapped fields.
    let projection = project(&ReleaseFacts {
        sort_title: Some("Tracers".into()),
        description: Some("A game.".into()),
        release_date: Some(CalendarDate::Complete {
            year: 1994,
            month: 3,
            day: 17,
        }),
        developers: vec!["Studio A".into()],
        publishers: vec!["Publisher B".into()],
        primary_genre: Some("Puzzle".into()),
        players: Some(PlayerCount::Exact(2)),
        region: Some("USA".into()),
        language: Some("En".into()),
        revision: Some("Rev A".into()),
        representation: Some("headered".into()),
        local_label: Some("my copy".into()),
        ..facts()
    });

    let mut owned = projection.owned_field_names();
    owned.sort_unstable();
    assert_eq!(
        owned,
        vec![
            "desc",
            "developer",
            "genre",
            "name",
            "players",
            "publisher",
            "releasedate",
            "sortname"
        ],
        "region, language, revision, representation, and labels are disambiguation \
         inputs, not exported fields"
    );
}

#[test]
fn descriptions_are_only_line_ending_normalized() {
    let projection = project(&ReleaseFacts {
        description: Some("First line.\r\nSecond line.".into()),
        ..facts()
    });

    assert_eq!(projection.fields["desc"], "First line.\nSecond line.");
}

#[test]
fn colliding_titles_gain_the_minimum_distinction() {
    let releases = vec![
        ReleaseFacts {
            title: "Tracers".into(),
            region: Some("USA".into()),
            ..Default::default()
        },
        ReleaseFacts {
            title: "Tracers".into(),
            region: Some("Europe".into()),
            ..Default::default()
        },
    ];

    assert_eq!(
        disambiguate_titles(&releases),
        vec!["Tracers (USA)", "Tracers (Europe)"],
        "region alone separates them, so nothing further is added"
    );
}

#[test]
fn a_unique_title_is_left_alone() {
    let releases = vec![ReleaseFacts {
        title: "Tracers".into(),
        region: Some("USA".into()),
        ..Default::default()
    }];

    assert_eq!(
        disambiguate_titles(&releases),
        vec!["Tracers"],
        "over-qualifying a title nobody could confuse makes the list unreadable"
    );
}

#[test]
fn distinctions_are_added_in_order_until_the_collision_clears() {
    let releases = vec![
        ReleaseFacts {
            title: "Tracers".into(),
            region: Some("USA".into()),
            revision: Some("Rev A".into()),
            ..Default::default()
        },
        ReleaseFacts {
            title: "Tracers".into(),
            region: Some("USA".into()),
            revision: Some("Rev B".into()),
            ..Default::default()
        },
    ];

    // Region matches, so it escalates to revision.
    assert_eq!(
        disambiguate_titles(&releases),
        vec!["Tracers (USA) (Rev A)", "Tracers (USA) (Rev B)"]
    );
}

#[test]
fn an_unresolvable_collision_uses_a_readable_local_label() {
    // A hash would be unique and useless to read.
    let releases = vec![
        ReleaseFacts {
            title: "Tracers".into(),
            local_label: Some("cart".into()),
            ..Default::default()
        },
        ReleaseFacts {
            title: "Tracers".into(),
            local_label: Some("download".into()),
            ..Default::default()
        },
    ];

    assert_eq!(
        disambiguate_titles(&releases),
        vec!["Tracers (cart)", "Tracers (download)"]
    );
}

#[test]
fn only_launchable_sets_receive_an_entry() {
    // Listing a track or a dependency would put entries in the user's library
    // that do nothing when selected.
    assert!(EntryEligibility::Launchable.gets_an_entry());
    for excluded in [
        EntryEligibility::ReferencedTrack,
        EntryEligibility::DiscRepresentedByPlaylist,
        EntryEligibility::Dependency,
        EntryEligibility::Directory,
        EntryEligibility::SourceContainer,
    ] {
        assert!(!excluded.gets_an_entry(), "{excluded:?} must not be listed");
    }
}
