mod common;

use std::time::Duration;

use common::{episode_payload, movie_payload};
use serde_json::json;
use watchkeep::plex::payload::{
    ParseResult, PlexEventName, PlexGuid, parse_ids, parse_plex_payload,
};
use watchkeep_storage::model::{ExternalIds, MediaRef};

fn guids(ids: &[&str]) -> Vec<PlexGuid> {
    ids.iter()
        .map(|id| PlexGuid {
            id: Some((*id).to_owned()),
        })
        .collect()
}

#[test]
fn reads_modern_plex_guids_and_the_guid_list() {
    let list = guids(&["imdb://tt1", "tmdb://2", "tvdb://3"]);
    let ids = parse_ids(Some("plex://movie/abc"), Some(&list));
    assert_eq!(
        ids,
        ExternalIds {
            plex_guid: Some("plex://movie/abc".to_owned()),
            imdb: Some("tt1".to_owned()),
            tmdb: Some("2".to_owned()),
            tvdb: Some("3".to_owned()),
        }
    );
}

#[test]
fn reads_legacy_agent_guids() {
    assert_eq!(
        parse_ids(Some("com.plexapp.agents.thetvdb://12345/1/5?lang=en"), None)
            .tvdb
            .as_deref(),
        Some("12345")
    );
    assert_eq!(
        parse_ids(Some("com.plexapp.agents.imdb://tt0113277?lang=en"), None)
            .imdb
            .as_deref(),
        Some("tt0113277")
    );
    assert_eq!(
        parse_ids(Some("com.plexapp.agents.themoviedb://949?lang=en"), None)
            .tmdb
            .as_deref(),
        Some("949")
    );
}

#[test]
fn parses_an_episode() {
    let ParseResult::Event(event) = parse_plex_payload(&episode_payload(json!({}), json!({})))
    else {
        panic!("expected an event");
    };
    assert_eq!(event.event, PlexEventName::Play);
    assert_eq!(event.account.as_deref(), Some("jayson"));
    assert_eq!(event.account_id.as_deref(), Some("1"));
    assert_eq!(event.view_offset, Some(Duration::from_secs(120)));
    let MediaRef::Episode(episode) = event.media else {
        panic!("expected an episode");
    };
    assert_eq!(episode.show.title, "Severance");
    assert_eq!(
        episode.show.ids.plex_guid.as_deref(),
        Some("plex://show/show1")
    );
    assert_eq!(episode.season, 1);
    assert_eq!(episode.number, 1);
    assert_eq!(episode.ids.tvdb.as_deref(), Some("371980"));
    assert_eq!(episode.duration, Some(Duration::from_secs(3_400)));
    assert_eq!(
        episode.aired_at.map(|date| date.to_string()).as_deref(),
        Some("2022-02-18")
    );
}

#[test]
fn parses_a_movie() {
    let ParseResult::Event(event) =
        parse_plex_payload(&movie_payload(json!({ "event": "media.stop" }), json!({})))
    else {
        panic!("expected an event");
    };
    assert_eq!(event.event, PlexEventName::Stop);
    let MediaRef::Movie(movie) = event.media else {
        panic!("expected a movie");
    };
    assert_eq!(movie.year, Some(1995));
    assert_eq!(movie.ids.imdb.as_deref(), Some("tt0113277"));
}

#[test]
fn derives_show_ids_from_a_legacy_episode_guid() {
    let payload = episode_payload(
        json!({}),
        json!({ "guid": "com.plexapp.agents.thetvdb://12345/1/5?lang=en", "grandparentGuid": null, "Guid": null }),
    );
    let ParseResult::Event(event) = parse_plex_payload(&payload) else {
        panic!("expected an event");
    };
    let MediaRef::Episode(episode) = event.media else {
        panic!("expected an episode");
    };
    assert_eq!(episode.show.ids.tvdb.as_deref(), Some("12345"));
}

#[test]
fn rejects_untracked_events_and_media_types() {
    let library = parse_plex_payload(
        &json!({ "event": "library.new", "Metadata": { "type": "movie", "title": "x" } }),
    );
    assert!(matches!(library, ParseResult::Ignored { .. }));
    let track = parse_plex_payload(&movie_payload(json!({}), json!({ "type": "track" })));
    assert!(matches!(track, ParseResult::Ignored { .. }));
    assert!(matches!(
        parse_plex_payload(&json!("nope")),
        ParseResult::Ignored { .. }
    ));
    assert!(matches!(
        parse_plex_payload(&json!({ "Metadata": {} })),
        ParseResult::Ignored { .. }
    ));
    let odd_shape = parse_plex_payload(
        &json!({ "event": "media.play", "Metadata": { "type": "movie", "title": "x", "year": "nineteen" } }),
    );
    assert!(
        matches!(odd_shape, ParseResult::Ignored { .. }),
        "a field with the wrong type is reported, not a crash"
    );
}

#[test]
fn reads_the_rating_for_media_rate() {
    let ParseResult::Event(event) = parse_plex_payload(&movie_payload(
        json!({ "event": "media.rate", "rating": 8 }),
        json!({}),
    )) else {
        panic!("expected an event");
    };
    assert_eq!(event.rating, Some(8.0));
}
