//! List and detail reads for the UI and the JSON API.
//!
//! The list queries take the filter and the sort as booleans, so that one static
//! query covers every combination and the macros can check it.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use eyre::Result;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::lists::{SortFlags, SortOrder, WatchFilter};
use crate::model::{MediaKind, PlayState, RatingKind, SPECIALS_SEASON, TargetKind};

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct MovieView {
    pub id: Uuid,
    pub kind: MediaKind,
    pub title: String,
    pub year: Option<i32>,
    pub plex_guid: Option<String>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub summary: Option<String>,
    pub poster_path: Option<String>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub play_count: i64,
    pub last_watched_at: Option<DateTime<Utc>>,
    pub rating: Option<f64>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShowView {
    pub id: Uuid,
    pub kind: MediaKind,
    pub title: String,
    pub year: Option<i32>,
    pub plex_guid: Option<String>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub summary: Option<String>,
    pub poster_path: Option<String>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Episodes with a local row.
    pub episode_count: i64,
    pub watched_count: i64,
    pub last_watched_at: Option<DateTime<Utc>>,
    pub rating: Option<f64>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct EpisodeView {
    pub id: Uuid,
    pub show_id: Uuid,
    pub season: i32,
    pub number: i32,
    pub title: Option<String>,
    pub plex_guid: Option<String>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub aired_at: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub play_count: i64,
    pub last_watched_at: Option<DateTime<Utc>>,
    pub position_ms: Option<i64>,
    pub progress_state: Option<PlayState>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    pub watched_at: DateTime<Utc>,
    pub source: String,
    pub account: Option<String>,
    pub player: Option<String>,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub show_id: Option<Uuid>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub number: Option<i32>,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ProgressView {
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    pub state: PlayState,
    pub account: Option<String>,
    pub player: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub title: Option<String>,
    pub show_id: Option<Uuid>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub number: Option<i32>,
    pub poster_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct WatchlistItem {
    pub kind: MediaKind,
    pub id: Uuid,
    pub title: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub listed_at: DateTime<Utc>,
    pub rank: Option<i32>,
    /// Plays for a movie, watched episodes for a show.
    pub watched_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Stats {
    pub movies: i64,
    pub movies_watched: i64,
    pub shows: i64,
    pub episodes: i64,
    pub episodes_watched: i64,
    pub plays: i64,
}

/// One library item with a TMDB id and the signals that weigh it in a taste
/// profile. An item with no play still appears, so that the recommendations
/// leave out what the library already holds.
///
/// Every count of a show leaves the specials of season 0 out, because the
/// catalog total that the profile compares them with leaves them out too. A
/// watched special would otherwise stand for an episode that nobody watched.
#[derive(Clone, Debug, PartialEq)]
pub struct TasteItem {
    pub id: Uuid,
    pub kind: MediaKind,
    pub tmdb_id: i64,
    pub title: String,
    /// Plays of the movie, or plays of the regular episodes of the show.
    pub play_count: i64,
    /// 1 for a watched movie, or the regular episodes of the show with a play.
    pub watched_count: i64,
    /// 1 for a movie, or the regular episodes of the show with a local row.
    pub episode_count: i64,
    /// The last play of the movie, or of a regular episode of the show.
    pub last_watched_at: Option<DateTime<Utc>>,
    /// The rating of the item. A show without a rating of its own takes the
    /// average of the ratings of its episodes, because Plex and Trakt rate
    /// episodes far more often than shows.
    pub rating: Option<f64>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct WebhookLogEntry {
    pub id: Uuid,
    pub received_at: DateTime<Utc>,
    pub event: String,
    pub account: Option<String>,
    pub player: Option<String>,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub outcome: String,
}

#[derive(Clone)]
pub struct Queries {
    pool: PgPool,
}

impl Queries {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The pool, for the aggregate reads in `statistics.rs`.
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn stats(&self) -> Result<Stats> {
        Ok(sqlx::query_as!(
            Stats,
            r#"SELECT
                 (SELECT COUNT(*) FROM media WHERE kind = $1) AS "movies!",
                 (SELECT COUNT(DISTINCT target_id) FROM plays WHERE target_kind = $2) AS "movies_watched!",
                 (SELECT COUNT(*) FROM media WHERE kind = $3) AS "shows!",
                 (SELECT COUNT(*) FROM episodes) AS "episodes!",
                 (SELECT COUNT(DISTINCT target_id) FROM plays WHERE target_kind = $4) AS "episodes_watched!",
                 (SELECT COUNT(*) FROM plays) AS "plays!""#,
            MediaKind::Movie.as_str(),
            TargetKind::Movie.as_str(),
            MediaKind::Show.as_str(),
            TargetKind::Episode.as_str()
        )
        .fetch_one(&self.pool)
        .await?)
    }

    // region: movies

    /// A `limit` of `None` returns every row.
    pub async fn movies(
        &self,
        filter: WatchFilter,
        search: &str,
        sort: SortOrder,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<MovieView>> {
        let (watched_only, unwatched_only) = filter.flags();
        let sort = SortFlags::from(sort);
        Ok(sqlx::query_as!(
            MovieView,
            r#"SELECT m.id, m.kind AS "kind: MediaKind", m.title, m.year, m.plex_guid, m.imdb_id, m.tmdb_id, m.tvdb_id,
                      m.duration_ms, m.summary, m.poster_path, m.hidden_at, m.created_at, m.updated_at,
                      COUNT(p.id) AS "play_count!",
                      MAX(p.watched_at) AS "last_watched_at?",
                      (SELECT r.rating FROM ratings r WHERE r.target_kind = $2 AND r.target_id = m.id) AS "rating?"
               FROM media m
               LEFT JOIN plays p ON p.target_kind = $2 AND p.target_id = m.id
               WHERE m.kind = $1 AND ($3::text = '' OR m.title ILIKE '%' || $3 || '%')
               GROUP BY m.id
               HAVING (NOT $4::bool OR COUNT(p.id) > 0) AND (NOT $5::bool OR COUNT(p.id) = 0)
               ORDER BY
                 CASE WHEN $6::bool THEN MAX(p.watched_at) END DESC NULLS LAST,
                 CASE WHEN $6::bool OR $9::bool THEN m.created_at END DESC,
                 CASE WHEN $8::bool THEN m.year END DESC NULLS LAST,
                 CASE WHEN $6::bool OR $7::bool OR $8::bool THEN lower(m.title) END,
                 CASE WHEN $7::bool THEN m.year END NULLS LAST,
                 CASE WHEN $9::bool THEN m.id END DESC,
                 m.id
               LIMIT $10 OFFSET $11"#,
            MediaKind::Movie.as_str(),
            TargetKind::Movie.as_str(),
            search,
            watched_only,
            unwatched_only,
            sort.recent,
            sort.title,
            sort.year,
            sort.added,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn movie_count(&self, filter: WatchFilter, search: &str) -> Result<i64> {
        let (watched_only, unwatched_only) = filter.flags();
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM (
                 SELECT m.id FROM media m
                 LEFT JOIN plays p ON p.target_kind = $2 AND p.target_id = m.id
                 WHERE m.kind = $1 AND ($3::text = '' OR m.title ILIKE '%' || $3 || '%')
                 GROUP BY m.id
                 HAVING (NOT $4::bool OR COUNT(p.id) > 0) AND (NOT $5::bool OR COUNT(p.id) = 0)
               ) matched"#,
            MediaKind::Movie.as_str(),
            TargetKind::Movie.as_str(),
            search,
            watched_only,
            unwatched_only
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn movie(&self, id: Uuid) -> Result<Option<MovieView>> {
        Ok(sqlx::query_as!(
            MovieView,
            r#"SELECT m.id, m.kind AS "kind: MediaKind", m.title, m.year, m.plex_guid, m.imdb_id, m.tmdb_id, m.tvdb_id,
                      m.duration_ms, m.summary, m.poster_path, m.hidden_at, m.created_at, m.updated_at,
                      COUNT(p.id) AS "play_count!",
                      MAX(p.watched_at) AS "last_watched_at?",
                      (SELECT r.rating FROM ratings r WHERE r.target_kind = $2 AND r.target_id = m.id) AS "rating?"
               FROM media m
               LEFT JOIN plays p ON p.target_kind = $2 AND p.target_id = m.id
               WHERE m.kind = $1 AND m.id = $3
               GROUP BY m.id"#,
            MediaKind::Movie.as_str(),
            TargetKind::Movie.as_str(),
            id
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    // endregion: movies

    // region: shows and episodes

    pub async fn shows(&self, search: &str, sort: SortOrder) -> Result<Vec<ShowView>> {
        let sort = SortFlags::from(sort);
        Ok(sqlx::query_as!(
            ShowView,
            r#"SELECT s.id, s.kind AS "kind: MediaKind", s.title, s.year, s.plex_guid, s.imdb_id, s.tmdb_id, s.tvdb_id,
                      s.duration_ms, s.summary, s.poster_path, s.hidden_at, s.created_at, s.updated_at,
                      COUNT(e.id) AS "episode_count!",
                      COALESCE(SUM(CASE WHEN EXISTS (SELECT 1 FROM plays p WHERE p.target_kind = $2 AND p.target_id = e.id) THEN 1 ELSE 0 END), 0)::bigint AS "watched_count!",
                      (SELECT MAX(p.watched_at) FROM plays p JOIN episodes e2 ON e2.id = p.target_id
                         WHERE p.target_kind = $2 AND e2.show_id = s.id) AS "last_watched_at?",
                      (SELECT r.rating FROM ratings r WHERE r.target_kind = $3 AND r.target_id = s.id) AS "rating?"
               FROM media s
               LEFT JOIN episodes e ON e.show_id = s.id
               WHERE s.kind = $1 AND ($4::text = '' OR s.title ILIKE '%' || $4 || '%')
               GROUP BY s.id
               ORDER BY
                 CASE WHEN $5::bool THEN (SELECT MAX(p.watched_at) FROM plays p JOIN episodes e2 ON e2.id = p.target_id
                                          WHERE p.target_kind = $2 AND e2.show_id = s.id) END DESC NULLS LAST,
                 CASE WHEN $5::bool OR $8::bool THEN s.created_at END DESC,
                 CASE WHEN $7::bool THEN s.year END DESC NULLS LAST,
                 CASE WHEN $5::bool OR $6::bool OR $7::bool THEN lower(s.title) END,
                 CASE WHEN $6::bool THEN s.year END NULLS LAST,
                 CASE WHEN $8::bool THEN s.id END DESC,
                 s.id"#,
            MediaKind::Show.as_str(),
            TargetKind::Episode.as_str(),
            RatingKind::Show.as_str(),
            search,
            sort.recent,
            sort.title,
            sort.year,
            sort.added
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn show(&self, id: Uuid) -> Result<Option<ShowView>> {
        Ok(sqlx::query_as!(
            ShowView,
            r#"SELECT s.id, s.kind AS "kind: MediaKind", s.title, s.year, s.plex_guid, s.imdb_id, s.tmdb_id, s.tvdb_id,
                      s.duration_ms, s.summary, s.poster_path, s.hidden_at, s.created_at, s.updated_at,
                      COUNT(e.id) AS "episode_count!",
                      COALESCE(SUM(CASE WHEN EXISTS (SELECT 1 FROM plays p WHERE p.target_kind = $2 AND p.target_id = e.id) THEN 1 ELSE 0 END), 0)::bigint AS "watched_count!",
                      (SELECT MAX(p.watched_at) FROM plays p JOIN episodes e2 ON e2.id = p.target_id
                         WHERE p.target_kind = $2 AND e2.show_id = s.id) AS "last_watched_at?",
                      (SELECT r.rating FROM ratings r WHERE r.target_kind = $3 AND r.target_id = s.id) AS "rating?"
               FROM media s
               LEFT JOIN episodes e ON e.show_id = s.id
               WHERE s.kind = $1 AND s.id = $4
               GROUP BY s.id"#,
            MediaKind::Show.as_str(),
            TargetKind::Episode.as_str(),
            RatingKind::Show.as_str(),
            id
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn episodes(&self, show_id: Uuid) -> Result<Vec<EpisodeView>> {
        Ok(sqlx::query_as!(
            EpisodeView,
            r#"SELECT e.id, e.show_id, e.season, e.number, e.title, e.plex_guid, e.imdb_id, e.tmdb_id, e.tvdb_id,
                      e.duration_ms, e.aired_at, e.created_at, e.updated_at,
                      COUNT(p.id) AS "play_count!",
                      MAX(p.watched_at) AS "last_watched_at?",
                      pr.position_ms AS "position_ms?",
                      pr.state AS "progress_state?: PlayState"
               FROM episodes e
               LEFT JOIN plays p ON p.target_kind = $2 AND p.target_id = e.id
               LEFT JOIN progress pr ON pr.target_kind = $2 AND pr.target_id = e.id
               WHERE e.show_id = $1
               GROUP BY e.id, pr.position_ms, pr.state
               ORDER BY e.season, e.number"#,
            show_id,
            TargetKind::Episode.as_str()
        )
        .fetch_all(&self.pool)
        .await?)
    }

    // endregion: shows and episodes

    // region: history and progress

    pub async fn history(&self, limit: i64, offset: i64) -> Result<Vec<HistoryEntry>> {
        Ok(sqlx::query_as!(
            HistoryEntry,
            r#"SELECT p.id, p.target_kind AS "target_kind: TargetKind", p.target_id, p.watched_at, p.source, p.account, p.player, p.external_id,
                      CASE p.target_kind WHEN $1 THEN m.title ELSE e.title END AS "title?",
                      CASE p.target_kind WHEN $1 THEN NULL ELSE s.id END AS "show_id?",
                      CASE p.target_kind WHEN $1 THEN NULL ELSE s.title END AS "show_title?",
                      e.season AS "season?",
                      e.number AS "number?",
                      CASE p.target_kind WHEN $1 THEN m.year ELSE s.year END AS "year?",
                      CASE p.target_kind WHEN $1 THEN m.poster_path ELSE s.poster_path END AS "poster_path?"
               FROM plays p
               LEFT JOIN media m ON p.target_kind = $1 AND m.id = p.target_id
               LEFT JOIN episodes e ON p.target_kind = $2 AND e.id = p.target_id
               LEFT JOIN media s ON s.id = e.show_id
               ORDER BY p.watched_at DESC, p.id DESC
               LIMIT $3 OFFSET $4"#,
            TargetKind::Movie.as_str(),
            TargetKind::Episode.as_str(),
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn history_count(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM plays"#)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn in_progress(&self) -> Result<Vec<ProgressView>> {
        Ok(sqlx::query_as!(
            ProgressView,
            r#"SELECT pr.target_kind AS "target_kind: TargetKind", pr.target_id, pr.position_ms, pr.duration_ms,
                      pr.state AS "state: PlayState", pr.account, pr.player, pr.updated_at,
                      CASE pr.target_kind WHEN $1 THEN m.title ELSE e.title END AS "title?",
                      CASE pr.target_kind WHEN $1 THEN NULL ELSE s.id END AS "show_id?",
                      CASE pr.target_kind WHEN $1 THEN NULL ELSE s.title END AS "show_title?",
                      e.season AS "season?",
                      e.number AS "number?",
                      CASE pr.target_kind WHEN $1 THEN m.poster_path ELSE s.poster_path END AS "poster_path?"
               FROM progress pr
               LEFT JOIN media m ON pr.target_kind = $1 AND m.id = pr.target_id
               LEFT JOIN episodes e ON pr.target_kind = $2 AND e.id = pr.target_id
               LEFT JOIN media s ON s.id = e.show_id
               ORDER BY pr.updated_at DESC"#,
            TargetKind::Movie.as_str(),
            TargetKind::Episode.as_str()
        )
        .fetch_all(&self.pool)
        .await?)
    }

    // endregion: history and progress

    // region: watchlist, lookups, and the webhook log

    pub async fn watchlist(&self) -> Result<Vec<WatchlistItem>> {
        Ok(sqlx::query_as!(
            WatchlistItem,
            r#"SELECT w.target_kind AS "kind: MediaKind", m.id, m.title, m.year, m.poster_path, w.listed_at, w.rank,
                      CASE w.target_kind
                        WHEN $1 THEN (SELECT COUNT(*) FROM plays p WHERE p.target_kind = $2 AND p.target_id = m.id)
                        ELSE (SELECT COUNT(DISTINCT p.target_id) FROM plays p JOIN episodes e ON e.id = p.target_id
                              WHERE p.target_kind = $3 AND e.show_id = m.id)
                      END AS "watched_count!"
               FROM watchlist w JOIN media m ON m.id = w.target_id
               ORDER BY w.rank NULLS LAST, w.listed_at"#,
            MediaKind::Movie.as_str(),
            TargetKind::Movie.as_str(),
            TargetKind::Episode.as_str()
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn is_on_watchlist(&self, kind: MediaKind, id: Uuid) -> Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM watchlist WHERE target_kind = $1 AND target_id = $2) AS "listed!""#,
            kind.as_str(),
            id
        )
        .fetch_one(&self.pool)
        .await?)
    }

    /// Local ids of media rows that carry one of the given TMDB ids.
    pub async fn local_by_tmdb(
        &self,
        kind: MediaKind,
        tmdb_ids: &[i64],
    ) -> Result<HashMap<i64, Uuid>> {
        if tmdb_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query!(
            r#"SELECT id, tmdb_id AS "tmdb_id!" FROM media WHERE kind = $1 AND tmdb_id = ANY($2)"#,
            kind.as_str(),
            tmdb_ids
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| (row.tmdb_id, row.id)).collect())
    }

    pub async fn recent_webhooks(&self, limit: i64) -> Result<Vec<WebhookLogEntry>> {
        Ok(sqlx::query_as!(
            WebhookLogEntry,
            "SELECT id, received_at, event, account, player, media_type, title, outcome
             FROM webhook_events ORDER BY received_at DESC, id DESC LIMIT $1",
            limit
        )
        .fetch_all(&self.pool)
        .await?)
    }

    // endregion: watchlist, lookups, and the webhook log

    // region: taste

    /// Every movie and show with a TMDB id, with its watch signals. The
    /// recommender turns these into a genre affinity and an exclusion list.
    pub async fn taste(&self) -> Result<Vec<TasteItem>> {
        let mut items = self.movie_taste().await?;
        items.extend(self.show_taste().await?);
        Ok(items)
    }

    async fn movie_taste(&self) -> Result<Vec<TasteItem>> {
        Ok(sqlx::query_as!(
            TasteItem,
            r#"SELECT m.id, m.kind AS "kind: MediaKind", m.tmdb_id AS "tmdb_id!", m.title,
                      COUNT(p.id) AS "play_count!",
                      LEAST(COUNT(p.id), 1) AS "watched_count!",
                      1::bigint AS "episode_count!",
                      MAX(p.watched_at) AS "last_watched_at?",
                      (SELECT r.rating FROM ratings r WHERE r.target_kind = $3 AND r.target_id = m.id) AS "rating?"
               FROM media m
               LEFT JOIN plays p ON p.target_kind = $2 AND p.target_id = m.id
               WHERE m.kind = $1 AND m.tmdb_id IS NOT NULL
               GROUP BY m.id"#,
            MediaKind::Movie.as_str(),
            TargetKind::Movie.as_str(),
            RatingKind::Movie.as_str()
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn show_taste(&self) -> Result<Vec<TasteItem>> {
        Ok(sqlx::query_as!(
            TasteItem,
            r#"SELECT s.id, s.kind AS "kind: MediaKind", s.tmdb_id AS "tmdb_id!", s.title,
                      (SELECT COUNT(*) FROM plays p JOIN episodes e2 ON e2.id = p.target_id
                         WHERE p.target_kind = $2 AND e2.show_id = s.id AND e2.season <> $5) AS "play_count!",
                      COALESCE(SUM(CASE WHEN EXISTS (SELECT 1 FROM plays p WHERE p.target_kind = $2 AND p.target_id = e.id) THEN 1 ELSE 0 END), 0)::bigint AS "watched_count!",
                      COUNT(e.id) AS "episode_count!",
                      (SELECT MAX(p.watched_at) FROM plays p JOIN episodes e2 ON e2.id = p.target_id
                         WHERE p.target_kind = $2 AND e2.show_id = s.id AND e2.season <> $5) AS "last_watched_at?",
                      COALESCE(
                        (SELECT r.rating FROM ratings r WHERE r.target_kind = $3 AND r.target_id = s.id),
                        (SELECT AVG(r.rating) FROM ratings r JOIN episodes e2 ON e2.id = r.target_id
                           WHERE r.target_kind = $4 AND e2.show_id = s.id)
                      ) AS "rating?"
               FROM media s
               LEFT JOIN episodes e ON e.show_id = s.id AND e.season <> $5
               WHERE s.kind = $1 AND s.tmdb_id IS NOT NULL
               GROUP BY s.id"#,
            MediaKind::Show.as_str(),
            TargetKind::Episode.as_str(),
            RatingKind::Show.as_str(),
            RatingKind::Episode.as_str(),
            SPECIALS_SEASON
        )
        .fetch_all(&self.pool)
        .await?)
    }

    // endregion: taste
}
