//! Manual changes that the UI and the JSON API both expose.

use chrono::{DateTime, NaiveDate, Utc};
use eyre::Result;
use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};
use tracing::instrument;
use uuid::Uuid;
use watchkeep_catalog::Catalog;
use watchkeep_storage::clock::{SharedClock, date_of};
use watchkeep_storage::library::{Library, PlayInput};
use watchkeep_storage::model::{
    EpisodeInput, ExternalIds, MediaKind, MediaRow, MovieRef, PlaySource, ShowRef, TargetKind,
};

use crate::scrobble::{enrich_movie, enrich_show, runtime_to_duration};
use crate::views::{SPECIALS_SEASON, has_aired};

#[derive(Clone, Debug)]
pub struct AddMediaInput {
    pub kind: MediaKind,
    pub tmdb_id: Option<i64>,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub watchlist: bool,
}

#[derive(Clone)]
pub struct Actions {
    pool: PgPool,
    catalog: Option<Catalog>,
    clock: SharedClock,
}

impl Actions {
    pub fn new(pool: PgPool, catalog: Option<Catalog>, clock: SharedClock) -> Self {
        Self {
            pool,
            catalog,
            clock,
        }
    }

    async fn library(&self) -> Result<Library<PoolConnection<Postgres>>> {
        Ok(Library::new(self.pool.acquire().await?, self.clock.clone()))
    }

    async fn get_show(&self, show_id: Uuid) -> Result<Option<MediaRow>> {
        let mut library = self.library().await?;
        Ok(library
            .get_media(show_id)
            .await?
            .filter(|media| media.kind == MediaKind::Show))
    }

    #[instrument(skip(self), err)]
    pub async fn mark_watched(
        &self,
        kind: TargetKind,
        id: Uuid,
        watched_at: Option<DateTime<Utc>>,
    ) -> Result<bool> {
        if !self.exists(kind, id).await? {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        {
            let mut library = Library::new(&mut *tx, self.clock.clone());
            library
                .record_play(PlayInput {
                    kind,
                    id,
                    watched_at,
                    source: PlaySource::Manual,
                    account: None,
                    player: None,
                    external_id: None,
                })
                .await?;
            library.clear_progress(kind, id).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    #[instrument(skip(self), err)]
    pub async fn mark_unwatched(&self, kind: TargetKind, id: Uuid) -> Result<bool> {
        if !self.exists(kind, id).await? {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        {
            let mut library = Library::new(&mut *tx, self.clock.clone());
            library.remove_plays(kind, id).await?;
            library.clear_progress(kind, id).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Mark an episode by season and number. Creates the local episode row when
    /// the episode is known only through the catalog.
    #[instrument(skip(self), err)]
    pub async fn mark_episode_by_number(
        &self,
        show_id: Uuid,
        season: i32,
        number: i32,
        watched: bool,
    ) -> Result<bool> {
        let Some(show) = self.get_show(show_id).await? else {
            return Ok(false);
        };
        let mut tx = self.pool.begin().await?;
        let episode = {
            let mut library = Library::new(&mut *tx, self.clock.clone());
            match library.find_episode(show_id, season, number).await? {
                Some(current) => current,
                None => {
                    let detail = match (show.tmdb_id.filter(|id| *id != 0), &self.catalog) {
                        (Some(tmdb_id), Some(catalog)) => {
                            catalog.episode(tmdb_id, season, number).await?
                        }
                        _ => None,
                    };
                    let input = EpisodeInput {
                        season,
                        number,
                        title: detail.as_ref().and_then(|detail| detail.title.clone()),
                        ids: ExternalIds {
                            tmdb: detail.as_ref().map(|detail| detail.tmdb_id.to_string()),
                            ..ExternalIds::default()
                        },
                        duration: runtime_to_duration(
                            detail.as_ref().and_then(|detail| detail.runtime_min),
                        ),
                        aired_at: detail.as_ref().and_then(|detail| detail.air_date),
                    };
                    library.upsert_episode(show_id, &input).await?
                }
            }
        };
        tx.commit().await?;
        if watched {
            self.mark_watched(TargetKind::Episode, episode.id, None)
                .await
        } else {
            self.mark_unwatched(TargetKind::Episode, episode.id).await
        }
    }

    /// Add one play to every known episode of the show that has no play yet.
    /// Returns the number of changed episodes, or `None` when the show does not exist.
    #[instrument(skip(self), err)]
    pub async fn mark_show_watched(
        &self,
        show_id: Uuid,
        today: Option<NaiveDate>,
    ) -> Result<Option<i64>> {
        let Some(show) = self.get_show(show_id).await? else {
            return Ok(None);
        };
        let mut tx = self.pool.begin().await?;
        let changed = {
            let mut library = Library::new(&mut *tx, self.clock.clone());
            let mut changed = 0;
            if let (Some(tmdb_id), Some(catalog)) =
                (show.tmdb_id.filter(|id| *id != 0), &self.catalog)
            {
                let cutoff = today.unwrap_or_else(|| date_of(self.clock.now()));
                for detail in catalog.episodes(tmdb_id).await? {
                    if detail.season == SPECIALS_SEASON || !has_aired(detail.air_date, cutoff) {
                        continue;
                    }
                    library
                        .upsert_episode(
                            show_id,
                            &EpisodeInput {
                                season: detail.season,
                                number: detail.number,
                                title: detail.title.clone(),
                                ids: ExternalIds {
                                    tmdb: Some(detail.tmdb_id.to_string()),
                                    ..ExternalIds::default()
                                },
                                duration: runtime_to_duration(detail.runtime_min),
                                aired_at: detail.air_date,
                            },
                        )
                        .await?;
                }
            }
            for episode in library.list_episodes(show_id).await? {
                if library.play_count(TargetKind::Episode, episode.id).await? > 0 {
                    continue;
                }
                library
                    .record_play(PlayInput {
                        kind: TargetKind::Episode,
                        id: episode.id,
                        watched_at: None,
                        source: PlaySource::Manual,
                        account: None,
                        player: None,
                        external_id: None,
                    })
                    .await?;
                library
                    .clear_progress(TargetKind::Episode, episode.id)
                    .await?;
                changed += 1;
            }
            changed
        };
        tx.commit().await?;
        Ok(Some(changed))
    }

    /// Returns the number of removed plays, or `None` when the show does not exist.
    #[instrument(skip(self), err)]
    pub async fn mark_show_unwatched(&self, show_id: Uuid) -> Result<Option<i64>> {
        if self.get_show(show_id).await?.is_none() {
            return Ok(None);
        }
        let mut tx = self.pool.begin().await?;
        let changed = {
            let mut library = Library::new(&mut *tx, self.clock.clone());
            let mut changed: i64 = 0;
            for episode in library.list_episodes(show_id).await? {
                let removed = library
                    .remove_plays(TargetKind::Episode, episode.id)
                    .await?;
                changed += i64::try_from(removed).unwrap_or(i64::MAX);
                library
                    .clear_progress(TargetKind::Episode, episode.id)
                    .await?;
            }
            changed
        };
        tx.commit().await?;
        Ok(Some(changed))
    }

    /// Add a movie or show by hand. With a TMDB id and a catalog, the catalog
    /// supplies the title, year, and poster. Otherwise the title is required.
    /// An item that already exists is returned, not duplicated.
    #[instrument(skip(self), err)]
    pub async fn add_media(&self, input: AddMediaInput) -> Result<Option<MediaRow>> {
        let tmdb_id = input.tmdb_id.filter(|id| *id > 0);
        let mut title = input
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned);
        if title.is_none()
            && let (Some(tmdb_id), Some(catalog)) = (tmdb_id, &self.catalog)
        {
            title = match input.kind {
                MediaKind::Movie => catalog
                    .movie_by_tmdb_id(tmdb_id)
                    .await?
                    .map(|movie| movie.title),
                MediaKind::Show => catalog
                    .show_by_tmdb_id(tmdb_id)
                    .await?
                    .map(|show| show.title),
            };
        }
        let Some(title) = title else { return Ok(None) };
        let ids = ExternalIds {
            tmdb: tmdb_id.map(|id| id.to_string()),
            ..ExternalIds::default()
        };
        let mut library = self.library().await?;
        let row = match input.kind {
            MediaKind::Movie => {
                let movie = enrich_movie(
                    self.catalog.as_ref(),
                    MovieRef {
                        title,
                        year: input.year,
                        ids,
                        duration: None,
                        summary: None,
                        poster_path: None,
                    },
                )
                .await?;
                library.upsert_movie(&movie).await?
            }
            MediaKind::Show => {
                let show = enrich_show(
                    self.catalog.as_ref(),
                    ShowRef {
                        title,
                        year: input.year,
                        ids,
                        summary: None,
                        poster_path: None,
                    },
                    None,
                )
                .await?;
                library.upsert_show(&show).await?
            }
        };
        if input.watchlist {
            library
                .add_to_watchlist(input.kind, row.id, None, None)
                .await?;
        }
        Ok(Some(row))
    }

    #[instrument(skip(self), err)]
    pub async fn set_watchlist(&self, kind: MediaKind, id: Uuid, listed: bool) -> Result<bool> {
        let mut library = self.library().await?;
        match library.get_media(id).await? {
            Some(media) if media.kind == kind => {}
            _ => return Ok(false),
        }
        if listed {
            library.add_to_watchlist(kind, id, None, None).await?;
        } else {
            library.remove_from_watchlist(kind, id).await?;
        }
        Ok(true)
    }

    #[instrument(skip(self), err)]
    pub async fn set_hidden(&self, id: Uuid, hidden: bool) -> Result<bool> {
        let mut library = self.library().await?;
        if library.get_media(id).await?.is_none() {
            return Ok(false);
        }
        library.set_hidden(id, hidden, None).await?;
        Ok(true)
    }

    #[instrument(skip(self), err)]
    pub async fn remove_play(&self, play_id: Uuid) -> Result<bool> {
        self.library().await?.remove_play(play_id).await
    }

    #[instrument(skip(self), err)]
    pub async fn clear_progress(&self, kind: TargetKind, id: Uuid) -> Result<()> {
        self.library().await?.clear_progress(kind, id).await
    }

    async fn exists(&self, kind: TargetKind, id: Uuid) -> Result<bool> {
        let mut library = self.library().await?;
        Ok(match kind {
            TargetKind::Movie => library
                .get_media(id)
                .await?
                .is_some_and(|media| media.kind == MediaKind::Movie),
            TargetKind::Episode => library.get_episode(id).await?.is_some(),
        })
    }
}
