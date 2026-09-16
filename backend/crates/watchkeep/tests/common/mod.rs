//! Shared test helpers: fresh databases per test, a fake clock, sample payloads,
//! and request builders for the router.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Method, Request, Response, StatusCode};
use chrono::{DateTime, Duration, Utc};
use eyre::Result;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
use tower::ServiceExt;
use uuid::Uuid;
use watchkeep::app::{AppContext, SharedContext, build_router};
use watchkeep::config::Config;
use watchkeep::plex::payload::{PlexEvent, parse_plex_payload};
use watchkeep_catalog::schema::apply_schema;
use watchkeep_storage::clock::Clock;
use watchkeep_storage::db::{create_pool, migrate};
use watchkeep_storage::model::MediaKind;
use watchkeep_telemetry::testing::{Flusher, init_goodies};

/// The admin connection of the test server. The tests create and drop their own databases.
pub const TEST_DATABASE_URL_VAR: &str = "WATCHKEEP_TEST_DATABASE_URL";

/// The webhook form field that Plex fills.
pub const PAYLOAD_FIELD: &str = "payload";

pub const WEBHOOK_PATH: &str = "/webhook/plex";

/// The time the fake clock starts at.
pub const START: &str = "2026-01-01T12:00:00Z";

pub fn admin_url() -> String {
    std::env::var(TEST_DATABASE_URL_VAR).unwrap_or_else(|_| {
        panic!("{TEST_DATABASE_URL_VAR} is not set. Run the tests through `scripts/test-db.sh cargo test` (starts Postgres in Docker).")
    })
}

/// A time in a test, from ISO-8601 text.
pub fn at(text: &str) -> DateTime<Utc> {
    text.parse().expect("an ISO-8601 time")
}

/// An id that no row has.
pub fn missing_id() -> Uuid {
    Uuid::now_v7()
}

pub struct FakeClock {
    current: Mutex<DateTime<Utc>>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(at(START)),
        }
    }

    pub fn advance_minutes(&self, minutes: i64) {
        *self.current.lock().expect("clock lock") += Duration::minutes(minutes);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.current.lock().expect("clock lock")
    }
}

pub fn test_config(overrides: impl FnOnce(&mut Config)) -> Config {
    let mut config = Config::default();
    overrides(&mut config);
    config
}

/// The admin URL with another database name.
pub fn url_for(database: &str) -> String {
    let admin = admin_url();
    let (base, query) = match admin.split_once('?') {
        Some((base, query)) => (base.to_owned(), Some(query.to_owned())),
        None => (admin, None),
    };
    let scheme_end = base.find("://").map_or(0, |index| index + 3);
    let path_start = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |index| index + scheme_end);
    let mut url = format!("{}/{database}", &base[..path_start]);
    if let Some(query) = query {
        url.push('?');
        url.push_str(&query);
    }
    url
}

/// Run one statement on the admin connection, for example `CREATE DATABASE`.
pub async fn admin(sql: String) -> Result<()> {
    let pool = create_pool(&admin_url(), 1).await?;
    sqlx::raw_sql(AssertSqlSafe(sql)).execute(&pool).await?;
    pool.close().await;
    Ok(())
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn unique_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos() as u64;
    format!(
        "wk_{nanos:x}_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// An app context on two fresh databases: one for Watchkeep and, with `with_catalog`,
/// one seeded TMDB catalog. `close()` drops both.
pub struct TestContext {
    pub ctx: SharedContext,
    pub clock: Arc<FakeClock>,
    name: String,
    with_catalog: bool,
    /// The same layers as production. It flushes when the test ends.
    _telemetry: Flusher,
}

impl std::ops::Deref for TestContext {
    type Target = AppContext;

    fn deref(&self) -> &AppContext {
        &self.ctx
    }
}

impl TestContext {
    pub fn app(&self) -> Router {
        build_router(self.ctx.clone())
    }

    /// The id of the only movie or show with this title.
    pub async fn media_id(&self, kind: MediaKind, title: &str) -> Result<Uuid> {
        let mut library = self.library().await?;
        let row = library
            .find_media(kind, title, None, &Default::default())
            .await?
            .unwrap_or_else(|| panic!("no {kind} titled {title}"));
        Ok(row.id)
    }

    /// The id of the first show in the library.
    pub async fn first_show(&self) -> Result<Uuid> {
        let shows = self
            .queries
            .shows("", watchkeep_storage::lists::SortOrder::Recent)
            .await?;
        Ok(shows.first().expect("a show").id)
    }

    /// The ids of the episodes of a show, in season and number order.
    pub async fn episode_ids(&self, show_id: Uuid) -> Result<Vec<Uuid>> {
        Ok(self
            .queries
            .episodes(show_id)
            .await?
            .into_iter()
            .map(|episode| episode.id)
            .collect())
    }

    pub async fn close(self) -> Result<()> {
        self.ctx.close().await;
        admin(format!("DROP DATABASE IF EXISTS {}", self.name)).await?;
        if self.with_catalog {
            admin(format!("DROP DATABASE IF EXISTS {}_cat", self.name)).await?;
        }
        Ok(())
    }
}

pub async fn test_context(
    overrides: impl FnOnce(&mut Config),
    with_catalog: bool,
) -> Result<TestContext> {
    let _telemetry = init_goodies();
    let name = unique_name();
    admin(format!("CREATE DATABASE {name}")).await?;
    let pool = create_pool(&url_for(&name), 3).await?;
    migrate(&pool).await?;
    let mut catalog_pool = None;
    let mut catalog_url = String::new();
    if with_catalog {
        admin(format!("CREATE DATABASE {name}_cat")).await?;
        catalog_url = url_for(&format!("{name}_cat"));
        let pool = create_pool(&catalog_url, 2).await?;
        apply_schema(&pool).await?;
        seed_catalog(&pool).await?;
        catalog_pool = Some(pool);
    }
    let clock = Arc::new(FakeClock::new());
    let config = test_config(|config| {
        overrides(config);
        config.catalog_database_url = catalog_url;
    });
    let ctx = Arc::new(AppContext::new(
        pool,
        catalog_pool,
        config,
        Some(clock.clone()),
    ));
    Ok(TestContext {
        ctx,
        clock,
        name,
        with_catalog,
        _telemetry,
    })
}

/// A small sample catalog: Severance and two Michael Mann films.
/// The statements are test fixtures on a schema the storage crate does not own,
/// so they are plain SQL, not macros.
pub async fn seed_catalog(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        "INSERT INTO tmdb_show (id, imdb_id, tvdb_id, name_en, name_pt, first_air_date, number_of_seasons, number_of_episodes, poster_path_en, overview_en, fetched_at, raw_json)
         VALUES (95396, 'tt11280740', 371980, 'Severance', 'Ruptura', '2022-02-17', 2, 4, '/pPHpeI2X1qEd1CS1SeyrdhZ4qnT.jpg', 'Mark leads a team of office workers.', 0, '{}');
         INSERT INTO tmdb_season (id, show_id, season_number, name, fetched_at) VALUES
           (1000, 95396, 0, 'Specials', 0), (1001, 95396, 1, 'Season 1', 0), (1002, 95396, 2, 'Season 2', 0);
         INSERT INTO tmdb_episode (id, season_id, episode_number, name, air_date, runtime) VALUES
           (5995805, 1000, 1, 'Welcome to Lumon', '2021-12-15', 7),
           (1982925, 1001, 1, 'Good News About Hell', '2022-02-17', 57),
           (3396429, 1001, 2, 'Half Loop', '2022-02-17', 53),
           (3396430, 1001, 3, 'In Perpetuity', '2022-02-24', 56),
           (4000001, 1002, 1, 'Hello, Ms. Cobel', '2025-01-17', 60),
           (4000002, 1002, 2, 'Far From Home', '2099-01-01', 60);
         INSERT INTO tmdb_movie (id, imdb_id, title_en, title_pt, release_date, runtime, poster_path_en, fetched_at, raw_json) VALUES
           (949, 'tt0113277', 'Heat', 'Fogo Contra Fogo', '1995-12-15', 170, '/e09dLw1Ljtccd2P4NsuUvVtS5du.jpg', 0, '{}'),
           (4638, 'tt0369339', 'Collateral', 'Colateral', '2004-08-04', 120, '/collateral.jpg', 0, '{}');",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Copy the keys of `patch` into `target`. A `null` value removes the key, like an `undefined` spread in JS.
fn merge(target: &mut Value, patch: Value) {
    if let (Value::Object(target), Value::Object(patch)) = (target, patch) {
        for (key, value) in patch {
            if value.is_null() {
                target.remove(&key);
            } else {
                target.insert(key, value);
            }
        }
    }
}

pub fn episode_payload(overrides: Value, metadata: Value) -> Value {
    let mut payload = json!({
        "event": "media.play",
        "user": true,
        "owner": true,
        "Account": { "id": 1, "title": "jayson" },
        "Server": { "title": "home", "uuid": "s1" },
        "Player": { "title": "Plex Web", "uuid": "p1", "local": true },
        "Metadata": {
            "librarySectionType": "show",
            "type": "episode",
            "title": "Pilot",
            "grandparentTitle": "Severance",
            "grandparentGuid": "plex://show/show1",
            "parentGuid": "plex://season/season1",
            "guid": "plex://episode/ep1",
            "parentIndex": 1,
            "index": 1,
            "duration": 3_400_000,
            "viewOffset": 120_000,
            "originallyAvailableAt": "2022-02-18",
            "Guid": [{ "id": "imdb://tt11280740" }, { "id": "tmdb://1982925" }, { "id": "tvdb://371980" }]
        }
    });
    merge(&mut payload["Metadata"], metadata);
    merge(&mut payload, overrides);
    payload
}

pub fn movie_payload(overrides: Value, metadata: Value) -> Value {
    let mut payload = json!({
        "event": "media.play",
        "Account": { "id": 1, "title": "jayson" },
        "Player": { "title": "Living room TV" },
        "Metadata": {
            "librarySectionType": "movie",
            "type": "movie",
            "title": "Heat",
            "year": 1995,
            "guid": "plex://movie/heat",
            "duration": 10_000_000,
            "viewOffset": 0,
            "Guid": [{ "id": "imdb://tt0113277" }, { "id": "tmdb://949" }]
        }
    });
    merge(&mut payload["Metadata"], metadata);
    merge(&mut payload, overrides);
    payload
}

pub fn event(payload: &Value) -> PlexEvent {
    parse_plex_payload(payload)
        .event()
        .expect("a tracked event")
}

/// The multipart form that Plex posts, to `uri`.
pub fn multipart_at(uri: &str, payload: &Value) -> Request<Body> {
    let boundary = "----watchkeep-test-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{PAYLOAD_FIELD}\"\r\n\r\n{payload}\r\n--{boundary}--\r\n"
    );
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("request")
}

pub fn multipart(payload: &Value) -> Request<Body> {
    multipart_at(WEBHOOK_PATH, payload)
}

pub fn json_request(method: Method, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

pub fn request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request")
}

pub async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.expect("response")
}

pub async fn body_json(response: Response<Body>) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// Send a request and return the status and the JSON body.
pub async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = send(app, request).await;
    let status = response.status();
    (status, body_json(response).await)
}

pub async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    call(app, request(Method::GET, uri)).await
}

pub fn titles(items: &Value) -> Vec<String> {
    items
        .as_array()
        .expect("an array")
        .iter()
        .map(|item| item["title"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// The `id` field of a JSON object, as a UUID.
pub fn id_of(value: &Value) -> Uuid {
    value["id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .expect("a UUID id")
}
