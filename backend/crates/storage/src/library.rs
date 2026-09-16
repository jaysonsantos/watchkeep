//! Writes and single-row reads for media, episodes, plays, progress, ratings,
//! the watchlist, and the webhook log. Every method runs one or a few
//! statements on the given connection. Callers that need atomicity pass a
//! transaction.

use std::ops::DerefMut;
use std::time::Duration;

use chrono::{DateTime, Utc};
use eyre::Result;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::clock::SharedClock;
use crate::model::{
    EpisodeInput, EpisodeRow, ExternalIds, MediaKind, MediaRow, MovieRef, PlayRow, PlaySource,
    PlayState, ProgressRow, RatingKind, ShowRef, TargetKind, WatchlistRow, millis, new_id,
    non_empty, tmdb_number, to_millis,
};

/// The `event` column of a webhook log row when the payload had no event name.
pub const UNKNOWN_EVENT: &str = "unknown";

pub struct PlayInput {
    pub kind: TargetKind,
    pub id: Uuid,
    /// Defaults to the clock.
    pub watched_at: Option<DateTime<Utc>>,
    pub source: PlaySource,
    pub account: Option<String>,
    pub player: Option<String>,
    pub external_id: Option<String>,
}

pub struct ProgressInput {
    pub kind: TargetKind,
    pub id: Uuid,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub state: PlayState,
    pub account: Option<String>,
    pub player: Option<String>,
    /// Defaults to the clock.
    pub updated_at: Option<DateTime<Utc>>,
}

pub struct WebhookLog {
    pub event: Option<String>,
    pub account: Option<String>,
    pub player: Option<String>,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub outcome: String,
    pub payload: String,
}

/// The columns that movies and shows share, borrowed from a `MovieRef` or a `ShowRef`.
struct MediaFields<'a> {
    title: &'a str,
    year: Option<i32>,
    ids: &'a ExternalIds,
    duration: Option<Duration>,
    summary: Option<&'a str>,
    poster_path: Option<&'a str>,
}

impl MovieRef {
    fn fields(&self) -> MediaFields<'_> {
        MediaFields {
            title: &self.title,
            year: self.year,
            ids: &self.ids,
            duration: self.duration,
            summary: self.summary.as_deref(),
            poster_path: self.poster_path.as_deref(),
        }
    }
}

impl ShowRef {
    fn fields(&self) -> MediaFields<'_> {
        MediaFields {
            title: &self.title,
            year: self.year,
            ids: &self.ids,
            duration: None,
            summary: self.summary.as_deref(),
            poster_path: self.poster_path.as_deref(),
        }
    }
}

/// Data access on one connection: a pool connection, a transaction, or a plain `&mut PgConnection`.
pub struct Library<C> {
    conn: C,
    clock: SharedClock,
}

impl<C: DerefMut<Target = PgConnection>> Library<C> {
    pub fn new(conn: C, clock: SharedClock) -> Self {
        Self { conn, clock }
    }

    pub fn clock(&self) -> &SharedClock {
        &self.clock
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    pub fn conn(&mut self) -> &mut PgConnection {
        self.conn.deref_mut()
    }

    pub fn into_inner(self) -> C {
        self.conn
    }

    // region: media

    /// Item identity: Plex guid, then TMDB id, then TVDB id, then IMDb id, then title and year.
    pub async fn find_media(
        &mut self,
        kind: MediaKind,
        title: &str,
        year: Option<i32>,
        ids: &ExternalIds,
    ) -> Result<Option<MediaRow>> {
        if let Some(guid) = non_empty(ids.plex_guid.as_deref()) {
            let row = sqlx::query_as!(
                MediaRow,
                r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                          duration_ms, summary, poster_path, hidden_at, created_at, updated_at
                   FROM media WHERE kind = $1 AND plex_guid = $2"#,
                kind.as_str(),
                guid
            )
            .fetch_optional(self.conn())
            .await?;
            if row.is_some() {
                return Ok(row);
            }
        }
        if let Some(tmdb) = tmdb_number(ids.tmdb.as_deref()) {
            let row = sqlx::query_as!(
                MediaRow,
                r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                          duration_ms, summary, poster_path, hidden_at, created_at, updated_at
                   FROM media WHERE kind = $1 AND tmdb_id = $2"#,
                kind.as_str(),
                tmdb
            )
            .fetch_optional(self.conn())
            .await?;
            if row.is_some() {
                return Ok(row);
            }
        }
        if let Some(tvdb) = non_empty(ids.tvdb.as_deref()) {
            let row = sqlx::query_as!(
                MediaRow,
                r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                          duration_ms, summary, poster_path, hidden_at, created_at, updated_at
                   FROM media WHERE kind = $1 AND tvdb_id = $2"#,
                kind.as_str(),
                tvdb
            )
            .fetch_optional(self.conn())
            .await?;
            if row.is_some() {
                return Ok(row);
            }
        }
        if let Some(imdb) = non_empty(ids.imdb.as_deref()) {
            let row = sqlx::query_as!(
                MediaRow,
                r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                          duration_ms, summary, poster_path, hidden_at, created_at, updated_at
                   FROM media WHERE kind = $1 AND imdb_id = $2"#,
                kind.as_str(),
                imdb
            )
            .fetch_optional(self.conn())
            .await?;
            if row.is_some() {
                return Ok(row);
            }
        }
        // A source that sends ids only leaves the title empty. Such an item
        // keeps its own row: a blank title matches every other blank title.
        let Some(title) = non_empty(Some(title.trim())) else {
            return Ok(None);
        };
        Ok(sqlx::query_as!(
            MediaRow,
            r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                      duration_ms, summary, poster_path, hidden_at, created_at, updated_at
               FROM media
               WHERE kind = $1 AND lower(title) = lower($2) AND (year IS NULL OR $3::int IS NULL OR year = $3)
               ORDER BY (year = $3) DESC NULLS LAST, created_at, id LIMIT 1"#,
            kind.as_str(),
            title,
            year
        )
        .fetch_optional(self.conn())
        .await?)
    }

    pub async fn get_media(&mut self, id: Uuid) -> Result<Option<MediaRow>> {
        Ok(sqlx::query_as!(
            MediaRow,
            r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                      duration_ms, summary, poster_path, hidden_at, created_at, updated_at
               FROM media WHERE id = $1"#,
            id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    async fn update_media(
        &mut self,
        current: &MediaRow,
        input: MediaFields<'_>,
    ) -> Result<MediaRow> {
        let now = self.now();
        Ok(sqlx::query_as!(
            MediaRow,
            r#"UPDATE media SET title = $1, year = $2, plex_guid = $3, imdb_id = $4, tmdb_id = $5, tvdb_id = $6,
                 duration_ms = $7, summary = $8, poster_path = $9, updated_at = $10
               WHERE id = $11
               RETURNING id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                         duration_ms, summary, poster_path, hidden_at, created_at, updated_at"#,
            input.title,
            input.year.or(current.year),
            input.ids.plex_guid.as_deref().or(current.plex_guid.as_deref()),
            input.ids.imdb.as_deref().or(current.imdb_id.as_deref()),
            tmdb_number(input.ids.tmdb.as_deref()).or(current.tmdb_id),
            input.ids.tvdb.as_deref().or(current.tvdb_id.as_deref()),
            millis(input.duration).or(current.duration_ms),
            input.summary.or(current.summary.as_deref()),
            input.poster_path.or(current.poster_path.as_deref()),
            now,
            current.id
        )
        .fetch_one(self.conn())
        .await?)
    }

    async fn insert_media(&mut self, kind: MediaKind, input: MediaFields<'_>) -> Result<MediaRow> {
        let now = self.now();
        Ok(sqlx::query_as!(
            MediaRow,
            r#"INSERT INTO media (id, kind, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
               RETURNING id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                         duration_ms, summary, poster_path, hidden_at, created_at, updated_at"#,
            new_id(now),
            kind.as_str(),
            input.title,
            input.year,
            input.ids.plex_guid.as_deref(),
            input.ids.imdb.as_deref(),
            tmdb_number(input.ids.tmdb.as_deref()),
            input.ids.tvdb.as_deref(),
            millis(input.duration),
            input.summary,
            input.poster_path,
            now
        )
        .fetch_one(self.conn())
        .await?)
    }

    pub async fn upsert_movie(&mut self, input: &MovieRef) -> Result<MediaRow> {
        match self
            .find_media(MediaKind::Movie, &input.title, input.year, &input.ids)
            .await?
        {
            Some(current) => self.update_media(&current, input.fields()).await,
            None => self.insert_media(MediaKind::Movie, input.fields()).await,
        }
    }

    pub async fn upsert_show(&mut self, input: &ShowRef) -> Result<MediaRow> {
        match self
            .find_media(MediaKind::Show, &input.title, input.year, &input.ids)
            .await?
        {
            Some(current) => self.update_media(&current, input.fields()).await,
            None => self.insert_media(MediaKind::Show, input.fields()).await,
        }
    }

    // endregion: media

    // region: episodes

    pub async fn get_episode(&mut self, id: Uuid) -> Result<Option<EpisodeRow>> {
        Ok(sqlx::query_as!(
            EpisodeRow,
            "SELECT id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at
             FROM episodes WHERE id = $1",
            id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    pub async fn find_episode(
        &mut self,
        show_id: Uuid,
        season: i32,
        number: i32,
    ) -> Result<Option<EpisodeRow>> {
        Ok(sqlx::query_as!(
            EpisodeRow,
            "SELECT id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at
             FROM episodes WHERE show_id = $1 AND season = $2 AND number = $3",
            show_id,
            season,
            number
        )
        .fetch_optional(self.conn())
        .await?)
    }

    pub async fn upsert_episode(
        &mut self,
        show_id: Uuid,
        input: &EpisodeInput,
    ) -> Result<EpisodeRow> {
        let by_guid = match non_empty(input.ids.plex_guid.as_deref()) {
            Some(guid) => {
                sqlx::query_as!(
                    EpisodeRow,
                    "SELECT id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at
                     FROM episodes WHERE plex_guid = $1",
                    guid
                )
                .fetch_optional(self.conn())
                .await?
            }
            None => None,
        };
        let current = match by_guid {
            Some(row) => Some(row),
            None => {
                self.find_episode(show_id, input.season, input.number)
                    .await?
            }
        };
        let now = self.now();
        if let Some(current) = current {
            return Ok(sqlx::query_as!(
                EpisodeRow,
                "UPDATE episodes SET show_id = $1, season = $2, number = $3, title = $4, plex_guid = $5, imdb_id = $6, tmdb_id = $7,
                   tvdb_id = $8, duration_ms = $9, aired_at = $10, updated_at = $11
                 WHERE id = $12
                 RETURNING id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at",
                show_id,
                input.season,
                input.number,
                input.title.as_deref().or(current.title.as_deref()),
                input.ids.plex_guid.as_deref().or(current.plex_guid.as_deref()),
                input.ids.imdb.as_deref().or(current.imdb_id.as_deref()),
                tmdb_number(input.ids.tmdb.as_deref()).or(current.tmdb_id),
                input.ids.tvdb.as_deref().or(current.tvdb_id.as_deref()),
                millis(input.duration).or(current.duration_ms),
                input.aired_at.or(current.aired_at),
                now,
                current.id
            )
            .fetch_one(self.conn())
            .await?);
        }
        Ok(sqlx::query_as!(
            EpisodeRow,
            "INSERT INTO episodes (id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
             RETURNING id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at",
            new_id(now),
            show_id,
            input.season,
            input.number,
            input.title.as_deref(),
            input.ids.plex_guid.as_deref(),
            input.ids.imdb.as_deref(),
            tmdb_number(input.ids.tmdb.as_deref()),
            input.ids.tvdb.as_deref(),
            millis(input.duration),
            input.aired_at,
            now
        )
        .fetch_one(self.conn())
        .await?)
    }

    pub async fn list_episodes(&mut self, show_id: Uuid) -> Result<Vec<EpisodeRow>> {
        Ok(sqlx::query_as!(
            EpisodeRow,
            "SELECT id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at
             FROM episodes WHERE show_id = $1 ORDER BY season, number",
            show_id
        )
        .fetch_all(self.conn())
        .await?)
    }

    // endregion: episodes

    // region: plays

    pub async fn last_play(&mut self, kind: TargetKind, id: Uuid) -> Result<Option<PlayRow>> {
        Ok(sqlx::query_as!(
            PlayRow,
            r#"SELECT id, target_kind AS "target_kind: TargetKind", target_id, watched_at, source, account, player, external_id
               FROM plays WHERE target_kind = $1 AND target_id = $2 ORDER BY watched_at DESC LIMIT 1"#,
            kind.as_str(),
            id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    pub async fn play_count(&mut self, kind: TargetKind, id: Uuid) -> Result<i64> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM plays WHERE target_kind = $1 AND target_id = $2"#,
            kind.as_str(),
            id
        )
        .fetch_one(self.conn())
        .await?)
    }

    /// The play that a source already recorded under this external id.
    pub async fn play_by_external_id(
        &mut self,
        source: PlaySource,
        external_id: &str,
    ) -> Result<Option<PlayRow>> {
        Ok(sqlx::query_as!(
            PlayRow,
            r#"SELECT id, target_kind AS "target_kind: TargetKind", target_id, watched_at, source, account, player, external_id
               FROM plays WHERE source = $1 AND external_id = $2"#,
            source.as_str(),
            external_id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    /// A play of the item inside `window` around `at`. Senders deliver events out
    /// of order, so the window looks forward and backward. A zero window matches nothing.
    pub async fn play_near(
        &mut self,
        kind: TargetKind,
        id: Uuid,
        at: DateTime<Utc>,
        window: Duration,
    ) -> Result<Option<PlayRow>> {
        let window = chrono::Duration::from_std(window)?;
        Ok(sqlx::query_as!(
            PlayRow,
            r#"SELECT id, target_kind AS "target_kind: TargetKind", target_id, watched_at, source, account, player, external_id
               FROM plays
               WHERE target_kind = $1 AND target_id = $2 AND watched_at > $3 AND watched_at < $4
               ORDER BY watched_at DESC LIMIT 1"#,
            kind.as_str(),
            id,
            at - window,
            at + window
        )
        .fetch_optional(self.conn())
        .await?)
    }

    /// Insert a play. With `external_id`, a play that already exists for the same
    /// source and id is skipped and `None` is returned.
    pub async fn record_play(&mut self, input: PlayInput) -> Result<Option<PlayRow>> {
        let now = self.now();
        Ok(sqlx::query_as!(
            PlayRow,
            r#"INSERT INTO plays (id, target_kind, target_id, watched_at, source, account, player, external_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (source, external_id) WHERE external_id IS NOT NULL DO NOTHING
               RETURNING id, target_kind AS "target_kind: TargetKind", target_id, watched_at, source, account, player, external_id"#,
            new_id(now),
            input.kind.as_str(),
            input.id,
            input.watched_at.unwrap_or(now),
            input.source.as_str(),
            input.account,
            input.player,
            input.external_id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    pub async fn remove_plays(&mut self, kind: TargetKind, id: Uuid) -> Result<u64> {
        Ok(sqlx::query!(
            "DELETE FROM plays WHERE target_kind = $1 AND target_id = $2",
            kind.as_str(),
            id
        )
        .execute(self.conn())
        .await?
        .rows_affected())
    }

    pub async fn remove_play(&mut self, play_id: Uuid) -> Result<bool> {
        Ok(sqlx::query!("DELETE FROM plays WHERE id = $1", play_id)
            .execute(self.conn())
            .await?
            .rows_affected()
            > 0)
    }

    // endregion: plays

    // region: progress

    pub async fn get_progress(
        &mut self,
        kind: TargetKind,
        id: Uuid,
    ) -> Result<Option<ProgressRow>> {
        Ok(sqlx::query_as!(
            ProgressRow,
            r#"SELECT target_kind AS "target_kind: TargetKind", target_id, position_ms, duration_ms, state AS "state: PlayState",
                      account, player, updated_at
               FROM progress WHERE target_kind = $1 AND target_id = $2"#,
            kind.as_str(),
            id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    pub async fn set_progress(&mut self, input: ProgressInput) -> Result<ProgressRow> {
        let updated_at = input.updated_at.unwrap_or_else(|| self.now());
        Ok(sqlx::query_as!(
            ProgressRow,
            r#"INSERT INTO progress (target_kind, target_id, position_ms, duration_ms, state, account, player, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (target_kind, target_id) DO UPDATE SET
                 position_ms = EXCLUDED.position_ms,
                 duration_ms = COALESCE(EXCLUDED.duration_ms, progress.duration_ms),
                 state = EXCLUDED.state,
                 account = EXCLUDED.account,
                 player = EXCLUDED.player,
                 updated_at = EXCLUDED.updated_at
               RETURNING target_kind AS "target_kind: TargetKind", target_id, position_ms, duration_ms, state AS "state: PlayState",
                         account, player, updated_at"#,
            input.kind.as_str(),
            input.id,
            to_millis(input.position),
            millis(input.duration),
            input.state.as_str(),
            input.account,
            input.player,
            updated_at
        )
        .fetch_one(self.conn())
        .await?)
    }

    /// Save the position, but keep a stored position that is newer than
    /// `updated_at`. `None` means the stored position won and nothing changed.
    /// Two requests for one item can overlap, so the comparison belongs in the
    /// statement, not between a read and a write.
    pub async fn set_progress_if_newer(
        &mut self,
        input: ProgressInput,
    ) -> Result<Option<ProgressRow>> {
        let updated_at = input.updated_at.unwrap_or_else(|| self.now());
        Ok(sqlx::query_as!(
            ProgressRow,
            r#"INSERT INTO progress (target_kind, target_id, position_ms, duration_ms, state, account, player, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (target_kind, target_id) DO UPDATE SET
                 position_ms = EXCLUDED.position_ms,
                 duration_ms = COALESCE(EXCLUDED.duration_ms, progress.duration_ms),
                 state = EXCLUDED.state,
                 account = EXCLUDED.account,
                 player = EXCLUDED.player,
                 updated_at = EXCLUDED.updated_at
               WHERE progress.updated_at <= EXCLUDED.updated_at
               RETURNING target_kind AS "target_kind: TargetKind", target_id, position_ms, duration_ms, state AS "state: PlayState",
                         account, player, updated_at"#,
            input.kind.as_str(),
            input.id,
            to_millis(input.position),
            millis(input.duration),
            input.state.as_str(),
            input.account,
            input.player,
            updated_at
        )
        .fetch_optional(self.conn())
        .await?)
    }

    /// Delete the position unless it is newer than `at`. A play clears the
    /// position, but never a position that a later event already wrote.
    pub async fn clear_progress_if_older(
        &mut self,
        kind: TargetKind,
        id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query!(
            "DELETE FROM progress WHERE target_kind = $1 AND target_id = $2 AND updated_at <= $3",
            kind.as_str(),
            id,
            at
        )
        .execute(self.conn())
        .await?;
        Ok(())
    }

    pub async fn clear_progress(&mut self, kind: TargetKind, id: Uuid) -> Result<()> {
        sqlx::query!(
            "DELETE FROM progress WHERE target_kind = $1 AND target_id = $2",
            kind.as_str(),
            id
        )
        .execute(self.conn())
        .await?;
        Ok(())
    }

    // endregion: progress

    // region: ratings

    pub async fn set_rating(
        &mut self,
        kind: RatingKind,
        id: Uuid,
        rating: f64,
        rated_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let rated_at = rated_at.unwrap_or_else(|| self.now());
        sqlx::query!(
            "INSERT INTO ratings (target_kind, target_id, rating, rated_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (target_kind, target_id) DO UPDATE SET rating = EXCLUDED.rating, rated_at = EXCLUDED.rated_at",
            kind.as_str(),
            id,
            rating,
            rated_at
        )
        .execute(self.conn())
        .await?;
        Ok(())
    }

    pub async fn get_rating(&mut self, kind: RatingKind, id: Uuid) -> Result<Option<f64>> {
        Ok(sqlx::query_scalar!(
            "SELECT rating FROM ratings WHERE target_kind = $1 AND target_id = $2",
            kind.as_str(),
            id
        )
        .fetch_optional(self.conn())
        .await?)
    }

    // endregion: ratings

    // region: watchlist and hidden

    pub async fn add_to_watchlist(
        &mut self,
        kind: MediaKind,
        id: Uuid,
        listed_at: Option<DateTime<Utc>>,
        rank: Option<i32>,
    ) -> Result<()> {
        let listed_at = listed_at.unwrap_or_else(|| self.now());
        sqlx::query!(
            "INSERT INTO watchlist (target_kind, target_id, listed_at, rank) VALUES ($1, $2, $3, $4)
             ON CONFLICT (target_kind, target_id) DO UPDATE SET rank = COALESCE(EXCLUDED.rank, watchlist.rank)",
            kind.as_str(),
            id,
            listed_at,
            rank
        )
        .execute(self.conn())
        .await?;
        Ok(())
    }

    pub async fn remove_from_watchlist(&mut self, kind: MediaKind, id: Uuid) -> Result<bool> {
        Ok(sqlx::query!(
            "DELETE FROM watchlist WHERE target_kind = $1 AND target_id = $2",
            kind.as_str(),
            id
        )
        .execute(self.conn())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn list_watchlist(&mut self) -> Result<Vec<WatchlistRow>> {
        Ok(sqlx::query_as!(
            WatchlistRow,
            r#"SELECT target_kind AS "target_kind: MediaKind", target_id, listed_at, rank
               FROM watchlist ORDER BY rank NULLS LAST, listed_at"#
        )
        .fetch_all(self.conn())
        .await?)
    }

    pub async fn set_hidden(
        &mut self,
        id: Uuid,
        hidden: bool,
        at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let now = self.now();
        let hidden_at = if hidden {
            Some(at.unwrap_or(now))
        } else {
            None
        };
        sqlx::query!(
            "UPDATE media SET hidden_at = $1, updated_at = $2 WHERE id = $3",
            hidden_at,
            now,
            id
        )
        .execute(self.conn())
        .await?;
        Ok(())
    }

    // endregion: watchlist and hidden

    // region: webhook log

    pub async fn log_webhook(&mut self, input: WebhookLog) -> Result<()> {
        let now = self.now();
        sqlx::query!(
            "INSERT INTO webhook_events (id, received_at, event, account, player, media_type, title, outcome, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            new_id(now),
            now,
            input.event.as_deref().unwrap_or(UNKNOWN_EVENT),
            input.account,
            input.player,
            input.media_type,
            input.title,
            input.outcome,
            input.payload
        )
        .execute(self.conn())
        .await?;
        Ok(())
    }

    /// Delete events older than `retention`. A zero retention keeps every event.
    pub async fn prune_webhook_log(&mut self, retention: Duration) -> Result<u64> {
        if retention.is_zero() {
            return Ok(0);
        }
        let cutoff = self.now() - chrono::Duration::from_std(retention)?;
        Ok(
            sqlx::query!("DELETE FROM webhook_events WHERE received_at < $1", cutoff)
                .execute(self.conn())
                .await?
                .rows_affected(),
        )
    }

    // endregion: webhook log

    /// A row id that sorts after every existing row.
    pub fn new_id(&self) -> Uuid {
        new_id(self.now())
    }
}
