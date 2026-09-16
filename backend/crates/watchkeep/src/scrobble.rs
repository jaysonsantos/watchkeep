//! Turns a Plex event into progress or plays. Catalog enrichment lives here,
//! because the scrobbler, the library sync, and the manual actions share it.

use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

use eyre::Result;
use serde::Serialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use watchkeep_catalog::{Catalog, CatalogEpisode, CatalogMovie, CatalogShow};
use watchkeep_storage::clock::SharedClock;
use watchkeep_storage::library::{Library, PlayInput, ProgressInput};
use watchkeep_storage::model::{
    EpisodeInput, EpisodeRef, ExternalIds, MediaRef, MovieRef, PlaySource, PlayState, ShowRef,
    TargetKind, non_empty, tmdb_number, to_millis,
};
use watchkeep_storage::text_enum;

use crate::config::Config;
use crate::plex::payload::{PlexEvent, PlexEventName};

const SECONDS_PER_MINUTE: u64 = 60;

/// Percentages keep one decimal.
const PERCENT_DECIMALS: f64 = 10.0;

pub const FULL_PERCENT: f64 = 100.0;

text_enum! {
    /// What the scrobbler did with an event. The text is the `outcome` of the webhook log.
    ScrobbleAction {
        Progress => "progress",
        Play => "play",
        DuplicatePlay => "duplicate-play",
        AlreadyWatched => "already-watched",
        Rating => "rating",
        IgnoredAccount => "ignored-account",
    }
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ScrobbleResult {
    pub action: ScrobbleAction,
    pub target_kind: TargetKind,
    /// The movie or episode. Absent when the event was ignored.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_id: Option<Uuid>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub position_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub percent: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ResolvedTarget {
    pub kind: TargetKind,
    pub id: Uuid,
    pub title: String,
    pub duration: Option<Duration>,
    pub show_id: Option<Uuid>,
}

/// A catalog runtime in minutes as a duration. Zero and absent runtimes give `None`.
pub fn runtime_to_duration(minutes: Option<i32>) -> Option<Duration> {
    minutes
        .and_then(|minutes| u64::try_from(minutes).ok())
        .filter(|minutes| *minutes != 0)
        .map(|minutes| Duration::from_secs(minutes * SECONDS_PER_MINUTE))
}

pub fn episode_code(season: i32, number: i32) -> String {
    format!("S{season:02}E{number:02}")
}

fn with_tmdb(
    ids: &ExternalIds,
    tmdb_id: Option<i64>,
    imdb: Option<&str>,
    tvdb: Option<&str>,
) -> ExternalIds {
    ExternalIds {
        plex_guid: ids.plex_guid.clone(),
        tmdb: ids
            .tmdb
            .clone()
            .or_else(|| tmdb_id.map(|id| id.to_string())),
        imdb: ids.imdb.clone().or_else(|| imdb.map(str::to_owned)),
        tvdb: ids.tvdb.clone().or_else(|| tvdb.map(str::to_owned)),
    }
}

/// Merge catalog data into a movie reference. Plex values win, the catalog fills gaps.
pub fn movie_with_catalog(media: MovieRef, matched: Option<&CatalogMovie>) -> MovieRef {
    let Some(matched) = matched else { return media };
    MovieRef {
        ids: with_tmdb(
            &media.ids,
            Some(matched.tmdb_id),
            matched.imdb_id.as_deref(),
            None,
        ),
        year: media.year.or(matched.year),
        duration: media
            .duration
            .or_else(|| runtime_to_duration(matched.runtime_min)),
        summary: media.summary.or_else(|| matched.overview.clone()),
        poster_path: matched.poster_path.clone(),
        title: media.title,
    }
}

pub fn show_with_catalog(show: ShowRef, matched: Option<&CatalogShow>) -> ShowRef {
    let Some(matched) = matched else { return show };
    ShowRef {
        title: show.title,
        year: show.year.or(matched.year),
        ids: with_tmdb(
            &show.ids,
            Some(matched.tmdb_id),
            matched.imdb_id.as_deref(),
            matched.tvdb_id.as_deref(),
        ),
        poster_path: matched.poster_path.clone(),
        summary: matched.overview.clone(),
    }
}

pub fn episode_with_catalog(
    media: EpisodeRef,
    show: ShowRef,
    episode: Option<&CatalogEpisode>,
) -> EpisodeRef {
    let input = media.episode;
    EpisodeRef {
        episode: EpisodeInput {
            title: input
                .title
                .or_else(|| episode.and_then(|episode| episode.title.clone())),
            ids: with_tmdb(
                &input.ids,
                episode.map(|episode| episode.tmdb_id),
                None,
                None,
            ),
            duration: input
                .duration
                .or_else(|| runtime_to_duration(episode.and_then(|episode| episode.runtime_min))),
            aired_at: input
                .aired_at
                .or_else(|| episode.and_then(|episode| episode.air_date)),
            season: input.season,
            number: input.number,
        },
        show,
    }
}

/// Fill in TMDB ids, posters, and runtimes from the catalog when it knows the item.
pub async fn enrich_movie(catalog: Option<&Catalog>, media: MovieRef) -> Result<MovieRef> {
    let Some(catalog) = catalog else {
        return Ok(media);
    };
    let mut matched = None;
    if let Some(tmdb_id) = tmdb_number(media.ids.tmdb.as_deref()) {
        matched = catalog.movie_by_tmdb_id(tmdb_id).await?;
    }
    if matched.is_none()
        && let Some(imdb) = non_empty(media.ids.imdb.as_deref())
    {
        matched = catalog.movie_by_imdb_id(imdb).await?;
    }
    if matched.is_none() {
        matched = catalog.movie_by_title(&media.title, media.year).await?;
    }
    Ok(movie_with_catalog(media, matched.as_ref()))
}

/// Find the catalog show for a show reference, or for the episode TMDB id when given.
pub async fn enrich_show(
    catalog: Option<&Catalog>,
    show: ShowRef,
    episode_tmdb_id: Option<&str>,
) -> Result<ShowRef> {
    let Some(catalog) = catalog else {
        return Ok(show);
    };
    let mut matched = None;
    if let Some(tmdb_id) = tmdb_number(show.ids.tmdb.as_deref()) {
        matched = catalog.show_by_tmdb_id(tmdb_id).await?;
    }
    if matched.is_none()
        && let Some(episode_tmdb) = tmdb_number(episode_tmdb_id)
    {
        matched = catalog
            .show_for_episode(episode_tmdb)
            .await?
            .map(|(show, _, _)| show);
    }
    if matched.is_none()
        && let Some(tvdb) = non_empty(show.ids.tvdb.as_deref())
    {
        matched = catalog.show_by_tvdb_id(tvdb).await?;
    }
    if matched.is_none()
        && let Some(imdb) = non_empty(show.ids.imdb.as_deref())
    {
        matched = catalog.show_by_imdb_id(imdb).await?;
    }
    if matched.is_none() {
        matched = catalog.show_by_name(&show.title, show.year).await?;
    }
    Ok(show_with_catalog(show, matched.as_ref()))
}

pub async fn enrich_episode(catalog: Option<&Catalog>, media: EpisodeRef) -> Result<EpisodeRef> {
    let Some(catalog) = catalog else {
        return Ok(media);
    };
    let show = enrich_show(Some(catalog), media.show.clone(), media.ids.tmdb.as_deref()).await?;
    let Some(show_tmdb) = tmdb_number(show.ids.tmdb.as_deref()) else {
        return Ok(EpisodeRef { show, ..media });
    };
    let episode = catalog
        .episode(show_tmdb, media.season, media.number)
        .await?;
    Ok(episode_with_catalog(media, show, episode.as_ref()))
}

/// Create or update the movie, show, and episode rows for a media reference.
pub async fn resolve_target<C: DerefMut<Target = PgConnection>>(
    library: &mut Library<C>,
    catalog: Option<&Catalog>,
    media: MediaRef,
) -> Result<ResolvedTarget> {
    match media {
        MediaRef::Movie(movie) => {
            let movie = library
                .upsert_movie(&enrich_movie(catalog, movie).await?)
                .await?;
            let title = match movie.year.filter(|year| *year != 0) {
                Some(year) => format!("{} ({year})", movie.title),
                None => movie.title.clone(),
            };
            Ok(ResolvedTarget {
                kind: TargetKind::Movie,
                id: movie.id,
                title,
                duration: movie.duration(),
                show_id: None,
            })
        }
        MediaRef::Episode(episode) => {
            let enriched = enrich_episode(catalog, episode).await?;
            let show = library.upsert_show(&enriched.show).await?;
            let episode = library.upsert_episode(show.id, &enriched).await?;
            let code = episode_code(episode.season, episode.number);
            let title = match non_empty(episode.title.as_deref()) {
                Some(name) => format!("{} {code} {name}", show.title),
                None => format!("{} {code}", show.title),
            };
            Ok(ResolvedTarget {
                kind: TargetKind::Episode,
                id: episode.id,
                title,
                duration: episode.duration(),
                show_id: Some(show.id),
            })
        }
    }
}

fn account_allowed(config: &Config, event: &PlexEvent) -> bool {
    let accepted = config.accepted_accounts();
    if accepted.is_empty() {
        return true;
    }
    accepted.iter().any(|allowed| {
        Some(*allowed) == event.account.as_deref() || Some(*allowed) == event.account_id.as_deref()
    })
}

/// The position as a percentage of the duration, with one decimal, capped at 100.
pub fn percent_of(position: Duration, duration: Option<Duration>) -> Option<f64> {
    let duration = duration.filter(|duration| !duration.is_zero())?;
    let ratio = position.as_secs_f64() / duration.as_secs_f64();
    let percent = (ratio * FULL_PERCENT * PERCENT_DECIMALS).round() / PERCENT_DECIMALS;
    Some(percent.min(FULL_PERCENT))
}

#[derive(Clone)]
pub struct Scrobbler {
    pool: PgPool,
    config: Arc<Config>,
    catalog: Option<Catalog>,
    clock: SharedClock,
}

impl Scrobbler {
    pub fn new(
        pool: PgPool,
        config: Arc<Config>,
        catalog: Option<Catalog>,
        clock: SharedClock,
    ) -> Self {
        Self {
            pool,
            config,
            catalog,
            clock,
        }
    }

    pub async fn apply(&self, event: &PlexEvent) -> Result<ScrobbleResult> {
        if !account_allowed(&self.config, event) {
            let kind = event.media.target_kind();
            return Ok(ScrobbleResult {
                action: ScrobbleAction::IgnoredAccount,
                target_kind: kind,
                target_id: None,
                title: kind.to_string(),
                position_ms: None,
                percent: None,
            });
        }
        let mut tx = self.pool.begin().await?;
        let result = {
            let mut library = Library::new(&mut *tx, self.clock.clone());
            let target =
                resolve_target(&mut library, self.catalog.as_ref(), event.media.clone()).await?;
            match event.event {
                PlexEventName::Rate => self.rate(&mut library, event, &target).await?,
                PlexEventName::Scrobble => {
                    self.record_play(&mut library, event, &target, PlaySource::PlexScrobble)
                        .await?
                }
                PlexEventName::Stop => self.stop(&mut library, event, &target).await?,
                PlexEventName::Pause => {
                    self.progress(&mut library, event, &target, PlayState::Paused)
                        .await?
                }
                PlexEventName::Play | PlexEventName::Resume => {
                    self.progress(&mut library, event, &target, PlayState::Playing)
                        .await?
                }
            }
        };
        tx.commit().await?;
        Ok(result)
    }

    async fn rate<C: DerefMut<Target = PgConnection>>(
        &self,
        library: &mut Library<C>,
        event: &PlexEvent,
        target: &ResolvedTarget,
    ) -> Result<ScrobbleResult> {
        if let Some(rating) = event.rating {
            library
                .set_rating(target.kind.into(), target.id, rating, None)
                .await?;
        }
        Ok(ScrobbleResult {
            action: ScrobbleAction::Rating,
            target_kind: target.kind,
            target_id: Some(target.id),
            title: target.title.clone(),
            position_ms: None,
            percent: None,
        })
    }

    /// A play of this target inside the rewatch window. Plex sends a late `stop` or
    /// `pause` after a `scrobble`, and both events must not undo the play.
    async fn played_recently<C: DerefMut<Target = PgConnection>>(
        &self,
        library: &mut Library<C>,
        target: &ResolvedTarget,
    ) -> Result<bool> {
        let last = library.last_play(target.kind, target.id).await?;
        let now = self.clock.now();
        Ok(last.is_some_and(|play| {
            (now - play.watched_at)
                .to_std()
                .is_ok_and(|since| since < self.config.rewatch_window)
        }))
    }

    /// The position and duration of an event: what Plex sent, else what the row has.
    async fn playback<C: DerefMut<Target = PgConnection>>(
        library: &mut Library<C>,
        event: &PlexEvent,
        target: &ResolvedTarget,
    ) -> Result<(Duration, Option<Duration>)> {
        let existing = library.get_progress(target.kind, target.id).await?;
        let position = event
            .view_offset
            .or_else(|| existing.as_ref().map(|progress| progress.position()))
            .unwrap_or_default();
        let duration = target
            .duration
            .or_else(|| existing.as_ref().and_then(|progress| progress.duration()));
        Ok((position, duration))
    }

    async fn progress<C: DerefMut<Target = PgConnection>>(
        &self,
        library: &mut Library<C>,
        event: &PlexEvent,
        target: &ResolvedTarget,
        state: PlayState,
    ) -> Result<ScrobbleResult> {
        if self.played_recently(library, target).await? {
            return Ok(ScrobbleResult {
                action: ScrobbleAction::AlreadyWatched,
                target_kind: target.kind,
                target_id: Some(target.id),
                title: target.title.clone(),
                position_ms: None,
                percent: None,
            });
        }
        let (position, duration) = Self::playback(library, event, target).await?;
        library
            .set_progress(ProgressInput {
                kind: target.kind,
                id: target.id,
                position,
                duration,
                state,
                account: event.account.clone(),
                player: event.player.clone(),
                updated_at: None,
            })
            .await?;
        Ok(ScrobbleResult {
            action: ScrobbleAction::Progress,
            target_kind: target.kind,
            target_id: Some(target.id),
            title: target.title.clone(),
            position_ms: Some(to_millis(position)),
            percent: percent_of(position, duration),
        })
    }

    async fn stop<C: DerefMut<Target = PgConnection>>(
        &self,
        library: &mut Library<C>,
        event: &PlexEvent,
        target: &ResolvedTarget,
    ) -> Result<ScrobbleResult> {
        let (position, duration) = Self::playback(library, event, target).await?;
        let finished = percent_of(position, duration)
            .is_some_and(|percent| percent >= self.config.watched_threshold_percent);
        if finished {
            return self
                .record_play(library, event, target, PlaySource::PlexStop)
                .await;
        }
        self.progress(library, event, target, PlayState::Stopped)
            .await
    }

    async fn record_play<C: DerefMut<Target = PgConnection>>(
        &self,
        library: &mut Library<C>,
        event: &PlexEvent,
        target: &ResolvedTarget,
        source: PlaySource,
    ) -> Result<ScrobbleResult> {
        let duplicate = self.played_recently(library, target).await?;
        library.clear_progress(target.kind, target.id).await?;
        if duplicate {
            return Ok(ScrobbleResult {
                action: ScrobbleAction::DuplicatePlay,
                target_kind: target.kind,
                target_id: Some(target.id),
                title: target.title.clone(),
                position_ms: None,
                percent: None,
            });
        }
        library
            .record_play(PlayInput {
                kind: target.kind,
                id: target.id,
                watched_at: None,
                source,
                account: event.account.clone(),
                player: event.player.clone(),
                external_id: None,
            })
            .await?;
        Ok(ScrobbleResult {
            action: ScrobbleAction::Play,
            target_kind: target.kind,
            target_id: Some(target.id),
            title: target.title.clone(),
            position_ms: None,
            percent: Some(FULL_PERCENT),
        })
    }
}
