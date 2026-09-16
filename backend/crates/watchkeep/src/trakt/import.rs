//! Import a Trakt data export (the ZIP from trakt.tv settings). Reads watch
//! history, ratings, playback positions, the watchlist, and hidden shows.
//! Every play carries the Trakt history id, so a second import adds nothing.
//!
//! The import collects every unique movie, show, and episode first and then
//! writes in bulk, so it needs a few dozen queries instead of one per record.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use eyre::{Result, WrapErr};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use tracing::{Span, field, instrument};
use uuid::Uuid;
use watchkeep_catalog::Catalog;
use watchkeep_storage::bulk::{
    EpisodeUpsert, MediaInput, PlayInsert, RatingInsert, bulk_record_plays, bulk_set_ratings,
    bulk_upsert_episodes, bulk_upsert_media,
};
use watchkeep_storage::clock::{SharedClock, parse_iso};
use watchkeep_storage::library::{Library, ProgressInput};
use watchkeep_storage::model::{
    EpisodeInput, EpisodeRef, EpisodeRow, ExternalIds, MediaKind, MediaRow, MovieRef, PlaySource,
    PlayState, RatingKind, ShowRef, TargetKind, non_empty, tmdb_number,
};

use crate::plex::payload::PLEX_GUID_SCHEME;
use crate::scrobble::{FULL_PERCENT, episode_with_catalog, movie_with_catalog, show_with_catalog};
use crate::telemetry::milliseconds;

/// The `player` of a playback position that came from Trakt.
const TRAKT_PLAYER: &str = "Trakt";

/// The file of the export that lists the endpoints Trakt failed to export.
const ERRORS_FILE: &str = "_errors.json";

const JSON_EXTENSION: &str = ".json";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraktImportReport {
    pub files: u64,
    pub plays: u64,
    pub plays_skipped: u64,
    pub movies: u64,
    pub shows: u64,
    pub episodes: u64,
    pub ratings: u64,
    pub playback: u64,
    pub playback_skipped: u64,
    pub watchlist: u64,
    pub hidden: u64,
    pub ignored_files: Vec<String>,
    pub export_errors: Vec<Value>,
    pub dry_run: bool,
}

/// The `type` of a Trakt record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TraktKind {
    Movie,
    Show,
    Episode,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct TraktPlex {
    guid: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct TraktIds {
    trakt: Option<i64>,
    imdb: Option<String>,
    tmdb: Option<i64>,
    tvdb: Option<i64>,
    plex: Option<TraktPlex>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct TraktMovie {
    title: Option<String>,
    year: Option<i32>,
    ids: Option<TraktIds>,
}

type TraktShow = TraktMovie;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct TraktEpisode {
    season: Option<i32>,
    number: Option<i32>,
    title: Option<String>,
    ids: Option<TraktIds>,
}

/// A record of any export file. Timestamps are ISO-8601 text.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct TraktRecord {
    id: Option<i64>,
    #[serde(rename = "type")]
    kind: TraktKind,
    watched_at: Option<String>,
    rated_at: Option<String>,
    rating: Option<f64>,
    /// Percent of the runtime.
    progress: Option<f64>,
    paused_at: Option<String>,
    listed_at: Option<String>,
    hidden_at: Option<String>,
    rank: Option<i32>,
    movie: Option<TraktMovie>,
    show: Option<TraktShow>,
    episode: Option<TraktEpisode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    History,
    Ratings,
    Playback,
    Watchlist,
    Hidden,
}

/// The export files that the import reads, by file name pattern.
static SECTIONS: LazyLock<Vec<(Regex, Section)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"^watched-history(-\d+)?\.json$").expect("valid regex"),
            Section::History,
        ),
        (
            Regex::new(r"^ratings-(movies|shows|episodes)(-\d+)?\.json$").expect("valid regex"),
            Section::Ratings,
        ),
        (
            Regex::new(r"^watched-playback(-\d+)?\.json$").expect("valid regex"),
            Section::Playback,
        ),
        (
            Regex::new(r"^lists-watchlist(-\d+)?\.json$").expect("valid regex"),
            Section::Watchlist,
        ),
        (
            Regex::new(r"^hidden-progress-watched(-\d+)?\.json$").expect("valid regex"),
            Section::Hidden,
        ),
    ]
});

/// `name-1.json`, `name-2.json`, … are one section split into numbered files.
static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.*?)(?:-(\d+))?\.json$").expect("valid regex"));

fn base_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

pub fn section_of(name: &str) -> Option<Section> {
    let base = base_name(name);
    SECTIONS
        .iter()
        .find(|(pattern, _)| pattern.is_match(base))
        .map(|(_, section)| *section)
}

fn plex_guid(kind: &str, guid: &str) -> String {
    format!("{PLEX_GUID_SCHEME}{kind}/{guid}")
}

fn ids(source: Option<&TraktIds>, plex_kind: &str) -> ExternalIds {
    let guid = source
        .and_then(|ids| ids.plex.as_ref())
        .and_then(|plex| non_empty(plex.guid.as_deref()));
    ExternalIds {
        plex_guid: guid.map(|guid| plex_guid(plex_kind, guid)),
        imdb: source
            .and_then(|ids| non_empty(ids.imdb.as_deref()))
            .map(str::to_owned),
        tmdb: source
            .and_then(|ids| ids.tmdb)
            .filter(|id| *id != 0)
            .map(|id| id.to_string()),
        tvdb: source
            .and_then(|ids| ids.tvdb)
            .filter(|id| *id != 0)
            .map(|id| id.to_string()),
    }
}

fn movie_key(movie: &TraktMovie) -> Option<String> {
    let title = non_empty(movie.title.as_deref())?;
    Some(
        match movie
            .ids
            .as_ref()
            .and_then(|ids| ids.trakt)
            .filter(|id| *id != 0)
        {
            Some(trakt) => format!("trakt:{trakt}"),
            None => format!(
                "title:{}:{}",
                title.to_lowercase(),
                movie.year.map(|year| year.to_string()).unwrap_or_default()
            ),
        },
    )
}

fn episode_key(episode: &TraktEpisode, show_key: &str) -> Option<String> {
    let (season, number) = (episode.season?, episode.number?);
    Some(
        match episode
            .ids
            .as_ref()
            .and_then(|ids| ids.trakt)
            .filter(|id| *id != 0)
        {
            Some(trakt) => format!("trakt:{trakt}"),
            None => format!("{show_key}:{season}:{number}"),
        },
    )
}

/// Sort `name-1.json`, `name-2.json`, … numerically.
fn sort_key(name: &str) -> (String, u64) {
    match NUMBERED.captures(name) {
        Some(captures) => (
            captures
                .get(1)
                .map(|m| m.as_str().to_owned())
                .unwrap_or_else(|| name.to_owned()),
            captures
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0),
        ),
        None => (name.to_owned(), 0),
    }
}

/// Read every `.json` entry of the ZIP as `(name, content)`, sorted by name and number.
pub fn read_export(bytes: &[u8]) -> Result<Vec<(String, String)>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).wrap_err("not a zip file")?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() || !base_name(file.name()).ends_with(JSON_EXTENSION) {
            continue;
        }
        let name = file.name().to_owned();
        let mut content = String::new();
        file.read_to_string(&mut content)
            .wrap_err_with(|| format!("cannot read {name}"))?;
        entries.push((name, content));
    }
    entries.sort_by_cached_key(|(name, _)| sort_key(name));
    Ok(entries)
}

/// Collected unique items from every file, keyed by Trakt id.
#[derive(Default)]
struct Collector {
    movies: IndexMap<String, MovieRef>,
    shows: IndexMap<String, ShowRef>,
    episodes: IndexMap<String, (String, EpisodeRef)>,
}

impl Collector {
    fn movie(&mut self, movie: Option<&TraktMovie>) -> Option<String> {
        let movie = movie?;
        let key = movie_key(movie)?;
        if !self.movies.contains_key(&key) {
            self.movies.insert(
                key.clone(),
                MovieRef {
                    title: movie.title.clone().unwrap_or_default(),
                    year: movie.year,
                    ids: ids(movie.ids.as_ref(), MediaKind::Movie.as_str()),
                    duration: None,
                    summary: None,
                    poster_path: None,
                },
            );
        }
        Some(key)
    }

    fn show(&mut self, show: Option<&TraktShow>) -> Option<String> {
        let show = show?;
        let key = movie_key(show)?;
        if !self.shows.contains_key(&key) {
            self.shows.insert(
                key.clone(),
                ShowRef {
                    title: show.title.clone().unwrap_or_default(),
                    year: show.year,
                    ids: ids(show.ids.as_ref(), MediaKind::Show.as_str()),
                    summary: None,
                    poster_path: None,
                },
            );
        }
        Some(key)
    }

    fn episode(
        &mut self,
        episode: Option<&TraktEpisode>,
        show: Option<&TraktShow>,
    ) -> Option<String> {
        let show_key = self.show(show)?;
        let episode = episode?;
        let key = episode_key(episode, &show_key)?;
        if !self.episodes.contains_key(&key) {
            let show_ref = self.shows[&show_key].clone();
            self.episodes.insert(
                key.clone(),
                (
                    show_key,
                    EpisodeRef {
                        episode: EpisodeInput {
                            title: episode.title.clone(),
                            season: episode.season.unwrap_or_default(),
                            number: episode.number.unwrap_or_default(),
                            ids: ids(episode.ids.as_ref(), TargetKind::Episode.as_str()),
                            duration: None,
                            aired_at: None,
                        },
                        show: show_ref,
                    },
                ),
            );
        }
        Some(key)
    }

    fn target(&mut self, record: &TraktRecord) -> Option<(TargetKind, String)> {
        match record.kind {
            TraktKind::Movie => self
                .movie(record.movie.as_ref())
                .map(|key| (TargetKind::Movie, key)),
            TraktKind::Episode => self
                .episode(record.episode.as_ref(), record.show.as_ref())
                .map(|key| (TargetKind::Episode, key)),
            TraktKind::Show | TraktKind::Other => None,
        }
    }
}

/// The key of a record's target without a write: the same keys `Collector::target` produces.
fn target_key(record: &TraktRecord) -> Option<(TargetKind, String)> {
    match record.kind {
        TraktKind::Movie => movie_key(record.movie.as_ref()?).map(|key| (TargetKind::Movie, key)),
        TraktKind::Episode => {
            let show_key = movie_key(record.show.as_ref()?)?;
            episode_key(record.episode.as_ref()?, &show_key).map(|key| (TargetKind::Episode, key))
        }
        TraktKind::Show | TraktKind::Other => None,
    }
}

async fn enrich_movies(
    catalog: Option<&Catalog>,
    movies: &mut IndexMap<String, MovieRef>,
) -> Result<()> {
    let Some(catalog) = catalog else {
        return Ok(());
    };
    let tmdb_ids: Vec<i64> = movies
        .values()
        .filter_map(|movie| tmdb_number(movie.ids.tmdb.as_deref()))
        .collect();
    let by_tmdb = catalog.movies_by_tmdb_ids(&tmdb_ids).await?;
    let imdb_ids: Vec<&str> = movies
        .values()
        .filter(|movie| {
            !tmdb_number(movie.ids.tmdb.as_deref()).is_some_and(|id| by_tmdb.contains_key(&id))
        })
        .filter_map(|movie| non_empty(movie.ids.imdb.as_deref()))
        .collect();
    let by_imdb = catalog.movies_by_imdb_ids(&imdb_ids).await?;
    for movie in movies.values_mut() {
        let mut matched = tmdb_number(movie.ids.tmdb.as_deref())
            .and_then(|id| by_tmdb.get(&id))
            .or_else(|| non_empty(movie.ids.imdb.as_deref()).and_then(|imdb| by_imdb.get(imdb)))
            .cloned();
        let has_id = non_empty(movie.ids.tmdb.as_deref()).is_some()
            || non_empty(movie.ids.imdb.as_deref()).is_some();
        if matched.is_none() && !has_id {
            matched = catalog.movie_by_title(&movie.title, movie.year).await?;
        }
        *movie = movie_with_catalog(movie.clone(), matched.as_ref());
    }
    Ok(())
}

async fn enrich_shows(
    catalog: Option<&Catalog>,
    shows: &mut IndexMap<String, ShowRef>,
) -> Result<()> {
    let Some(catalog) = catalog else {
        return Ok(());
    };
    let tmdb_ids: Vec<i64> = shows
        .values()
        .filter_map(|show| tmdb_number(show.ids.tmdb.as_deref()))
        .collect();
    let by_tmdb = catalog.shows_by_tmdb_ids(&tmdb_ids).await?;
    let unmatched: Vec<&ShowRef> = shows
        .values()
        .filter(|show| {
            !tmdb_number(show.ids.tmdb.as_deref()).is_some_and(|id| by_tmdb.contains_key(&id))
        })
        .collect();
    let tvdb_ids: Vec<&str> = unmatched
        .iter()
        .filter_map(|show| non_empty(show.ids.tvdb.as_deref()))
        .collect();
    let imdb_ids: Vec<&str> = unmatched
        .iter()
        .filter_map(|show| non_empty(show.ids.imdb.as_deref()))
        .collect();
    let by_tvdb = catalog.shows_by_tvdb_ids(&tvdb_ids).await?;
    let by_imdb = catalog.shows_by_imdb_ids(&imdb_ids).await?;
    for show in shows.values_mut() {
        let mut matched = tmdb_number(show.ids.tmdb.as_deref())
            .and_then(|id| by_tmdb.get(&id))
            .or_else(|| non_empty(show.ids.tvdb.as_deref()).and_then(|tvdb| by_tvdb.get(tvdb)))
            .or_else(|| non_empty(show.ids.imdb.as_deref()).and_then(|imdb| by_imdb.get(imdb)))
            .cloned();
        let has_id = non_empty(show.ids.tmdb.as_deref()).is_some()
            || non_empty(show.ids.tvdb.as_deref()).is_some()
            || non_empty(show.ids.imdb.as_deref()).is_some();
        if matched.is_none() && !has_id {
            matched = catalog.show_by_name(&show.title, show.year).await?;
        }
        *show = show_with_catalog(show.clone(), matched.as_ref());
    }
    Ok(())
}

async fn enrich_episodes(
    catalog: Option<&Catalog>,
    shows: &IndexMap<String, ShowRef>,
    episodes: &mut IndexMap<String, (String, EpisodeRef)>,
) -> Result<()> {
    let show_tmdb_ids: Vec<i64> = shows
        .values()
        .filter_map(|show| tmdb_number(show.ids.tmdb.as_deref()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let known = match catalog {
        Some(catalog) => catalog.episodes_for_shows(&show_tmdb_ids).await?,
        None => HashMap::new(),
    };
    for (show_key, episode) in episodes.values_mut() {
        let show = shows[show_key.as_str()].clone();
        let matched = tmdb_number(show.ids.tmdb.as_deref())
            .and_then(|tmdb| known.get(&(tmdb, episode.season, episode.number)));
        *episode = episode_with_catalog(episode.clone(), show, matched);
    }
    Ok(())
}

pub struct ImportOptions {
    pub clock: SharedClock,
    pub dry_run: bool,
}

struct Target {
    kind: TargetKind,
    id: Uuid,
    duration: Option<Duration>,
}

/// A timestamp of a record, or `None` when the record has none or it is not ISO-8601.
fn time_of(text: Option<&str>) -> Option<DateTime<Utc>> {
    non_empty(text).and_then(parse_iso)
}

/// Imports one Trakt export. The span holds what the import wrote, and the
/// metrics hold the time and the number of runs.
#[instrument(
    skip_all,
    err,
    fields(
        trakt.dry_run = options.dry_run,
        trakt.plays = field::Empty,
        trakt.ratings = field::Empty,
    )
)]
pub async fn import_trakt_export(
    pool: &PgPool,
    catalog: Option<&Catalog>,
    zip_path: &Path,
    options: ImportOptions,
) -> Result<TraktImportReport> {
    let started = Instant::now();
    let clock = options.clock;
    let now = clock.now();
    let bytes =
        std::fs::read(zip_path).wrap_err_with(|| format!("cannot read {}", zip_path.display()))?;
    let entries = read_export(&bytes)?;
    let mut report = TraktImportReport {
        dry_run: options.dry_run,
        ..TraktImportReport::default()
    };

    // Phase 1: read every file and collect unique items.
    let mut records: HashMap<Section, Vec<TraktRecord>> = HashMap::new();
    for (name, content) in &entries {
        let base = base_name(name);
        let parsed: Value =
            serde_json::from_str(content).wrap_err_with(|| format!("{base} is not JSON"))?;
        if base == ERRORS_FILE {
            if let Value::Array(errors) = parsed {
                report.export_errors = errors;
            }
            continue;
        }
        let section = section_of(base);
        let (Some(section), Value::Array(items)) = (section, parsed) else {
            report.ignored_files.push(base.to_owned());
            continue;
        };
        report.files += 1;
        let count = items.len();
        let parsed: Vec<TraktRecord> = serde_json::from_value(Value::Array(items))
            .wrap_err_with(|| format!("{base}: unexpected record shape"))?;
        records.entry(section).or_default().extend(parsed);
        tracing::info!("{base}: {count} records");
    }
    let section = |section: Section| records.get(&section).map(Vec::as_slice).unwrap_or(&[]);

    let mut collector = Collector::default();
    for record in section(Section::History) {
        collector.target(record);
    }
    for record in section(Section::Ratings) {
        if record.kind == TraktKind::Show {
            collector.show(record.show.as_ref());
        } else {
            collector.target(record);
        }
    }
    for record in section(Section::Playback) {
        collector.target(record);
    }
    for record in section(Section::Watchlist)
        .iter()
        .chain(section(Section::Hidden))
    {
        match record.kind {
            TraktKind::Show => {
                collector.show(record.show.as_ref());
            }
            TraktKind::Movie => {
                collector.movie(record.movie.as_ref());
            }
            TraktKind::Episode | TraktKind::Other => {}
        }
    }
    tracing::info!(
        "{} movies, {} shows, {} episodes to resolve",
        collector.movies.len(),
        collector.shows.len(),
        collector.episodes.len()
    );

    // Phase 2: catalog enrichment in bulk.
    enrich_movies(catalog, &mut collector.movies).await?;
    enrich_shows(catalog, &mut collector.shows).await?;
    enrich_episodes(catalog, &collector.shows, &mut collector.episodes).await?;

    // Phase 3: write everything in one transaction.
    let mut tx = pool.begin().await?;

    let movie_keys: Vec<&String> = collector.movies.keys().collect();
    let movie_inputs: Vec<MediaInput<'_>> =
        collector.movies.values().map(MediaInput::Movie).collect();
    let movie_rows: HashMap<String, MediaRow> = movie_keys
        .iter()
        .map(|key| (*key).clone())
        .zip(bulk_upsert_media(&mut tx, MediaKind::Movie, &movie_inputs, now).await?)
        .collect();
    report.movies = movie_rows
        .values()
        .map(|row| row.id)
        .collect::<HashSet<_>>()
        .len() as u64;

    let show_keys: Vec<&String> = collector.shows.keys().collect();
    let show_inputs: Vec<MediaInput<'_>> = collector.shows.values().map(MediaInput::Show).collect();
    let show_rows: HashMap<String, MediaRow> = show_keys
        .iter()
        .map(|key| (*key).clone())
        .zip(bulk_upsert_media(&mut tx, MediaKind::Show, &show_inputs, now).await?)
        .collect();
    report.shows = show_rows
        .values()
        .map(|row| row.id)
        .collect::<HashSet<_>>()
        .len() as u64;

    let episode_keys: Vec<&String> = collector.episodes.keys().collect();
    let episode_inputs: Vec<EpisodeUpsert<'_>> = collector
        .episodes
        .values()
        .map(|(show_key, episode)| EpisodeUpsert {
            show_id: show_rows[show_key].id,
            input: &episode.episode,
        })
        .collect();
    let episode_rows: HashMap<String, EpisodeRow> = episode_keys
        .iter()
        .map(|key| (*key).clone())
        .zip(bulk_upsert_episodes(&mut tx, &episode_inputs, now).await?)
        .collect();
    report.episodes = episode_rows
        .values()
        .map(|row| row.id)
        .collect::<HashSet<_>>()
        .len() as u64;

    let target_of = |record: &TraktRecord| -> Option<Target> {
        let (kind, key) = target_key(record)?;
        match kind {
            TargetKind::Movie => movie_rows.get(&key).map(|row| Target {
                kind,
                id: row.id,
                duration: row.duration(),
            }),
            TargetKind::Episode => episode_rows.get(&key).map(|row| Target {
                kind,
                id: row.id,
                duration: row.duration(),
            }),
        }
    };

    // Plays.
    let mut plays: Vec<PlayInsert> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for record in section(Section::History) {
        let target = target_of(record);
        let external_id = record.id.filter(|id| *id != 0).map(|id| id.to_string());
        let (Some(target), Some(watched_at)) = (target, time_of(record.watched_at.as_deref()))
        else {
            report.plays_skipped += 1;
            continue;
        };
        if let Some(external_id) = &external_id
            && !seen.insert(external_id.clone())
        {
            report.plays_skipped += 1;
            continue;
        }
        plays.push(PlayInsert {
            kind: target.kind,
            id: target.id,
            watched_at,
            source: PlaySource::Trakt,
            external_id,
            account: None,
            player: None,
        });
    }
    report.plays = bulk_record_plays(&mut tx, &plays, now).await?;
    report.plays_skipped += plays.len() as u64 - report.plays;

    // Ratings: the latest rating per item wins.
    let mut ratings: HashMap<(RatingKind, Uuid), RatingInsert> = HashMap::new();
    for record in section(Section::Ratings) {
        let Some(rating) = record.rating else {
            continue;
        };
        let target: Option<(RatingKind, Uuid)> = if record.kind == TraktKind::Show {
            record
                .show
                .as_ref()
                .and_then(movie_key)
                .and_then(|key| show_rows.get(&key))
                .map(|row| (RatingKind::Show, row.id))
        } else {
            target_of(record).map(|target| (target.kind.into(), target.id))
        };
        let Some((kind, id)) = target else { continue };
        let rated_at = time_of(record.rated_at.as_deref()).unwrap_or(now);
        match ratings.get(&(kind, id)) {
            Some(current) if current.rated_at >= rated_at => {}
            _ => {
                ratings.insert(
                    (kind, id),
                    RatingInsert {
                        kind,
                        id,
                        rating,
                        rated_at,
                    },
                );
            }
        }
    }
    let rating_inserts: Vec<RatingInsert> = ratings.into_values().collect();
    bulk_set_ratings(&mut tx, &rating_inserts).await?;
    report.ratings = rating_inserts.len() as u64;

    // Playback positions: need a runtime, and never overwrite a newer position.
    let mut library = Library::new(&mut *tx, clock.clone());
    for record in section(Section::Playback) {
        let (Some(target), Some(progress)) = (target_of(record), record.progress) else {
            report.playback_skipped += 1;
            continue;
        };
        let Some(duration) = target.duration.filter(|duration| !duration.is_zero()) else {
            report.playback_skipped += 1;
            continue;
        };
        let paused_at = time_of(record.paused_at.as_deref()).unwrap_or(now);
        if let Some(existing) = library.get_progress(target.kind, target.id).await?
            && existing.updated_at >= paused_at
        {
            report.playback_skipped += 1;
            continue;
        }
        library
            .set_progress(ProgressInput {
                kind: target.kind,
                id: target.id,
                position: duration.mul_f64(progress / FULL_PERCENT),
                duration: Some(duration),
                state: PlayState::Stopped,
                account: None,
                player: Some(TRAKT_PLAYER.to_owned()),
                updated_at: Some(paused_at),
            })
            .await?;
        report.playback += 1;
    }

    // Watchlist and hidden items.
    let media_of = |record: &TraktRecord| -> Option<(MediaKind, Uuid)> {
        match record.kind {
            TraktKind::Movie => movie_key(record.movie.as_ref()?)
                .and_then(|key| movie_rows.get(&key))
                .map(|row| (MediaKind::Movie, row.id)),
            TraktKind::Show => movie_key(record.show.as_ref()?)
                .and_then(|key| show_rows.get(&key))
                .map(|row| (MediaKind::Show, row.id)),
            TraktKind::Episode | TraktKind::Other => None,
        }
    };
    for record in section(Section::Watchlist) {
        let Some((kind, id)) = media_of(record) else {
            continue;
        };
        library
            .add_to_watchlist(kind, id, time_of(record.listed_at.as_deref()), record.rank)
            .await?;
        report.watchlist += 1;
    }
    for record in section(Section::Hidden) {
        let Some((_, id)) = media_of(record) else {
            continue;
        };
        library
            .set_hidden(id, true, time_of(record.hidden_at.as_deref()))
            .await?;
        report.hidden += 1;
    }
    drop(library);

    if options.dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    tracing::info!(
        "{}{} plays ({} skipped), {} ratings, {} positions ({} skipped), {} watchlist items, {} hidden",
        if options.dry_run { "dry run: " } else { "" },
        report.plays,
        report.plays_skipped,
        report.ratings,
        report.playback,
        report.playback_skipped,
        report.watchlist,
        report.hidden
    );
    let span = Span::current();
    span.record("trakt.plays", report.plays);
    span.record("trakt.ratings", report.ratings);
    tracing::info!(
        monotonic_counter.watchkeep_trakt_imports_total = 1_u64,
        histogram.watchkeep_trakt_import_duration_ms = milliseconds(started.elapsed()),
    );
    Ok(report)
}
