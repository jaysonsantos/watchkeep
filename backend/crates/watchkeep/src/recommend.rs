//! Recommendations from the watch history and the TMDB catalog.
//!
//! The watch history gives an implicit taste profile: a weight per watched item
//! from the plays, the replays, the share of a show that is watched, the age of
//! the last play, and an explicit rating when there is one. The weights add up
//! per genre, which gives one vector per genre kind. The catalog then scores its
//! own items against that vector.
//!
//! A list takes only a few items of one genre set and only a few movies of one
//! collection, because the score of the items of one group barely differs and
//! the best group would otherwise fill the whole list.
//!
//! The two databases stay apart. The storage database gives the ids and the
//! signals, the catalog database gives the genres and the candidates, and this
//! module joins the two results in memory.

use std::collections::HashMap;
use std::hash::Hash;

use chrono::{DateTime, Utc};
use eyre::Result;
use serde::Serialize;
use tracing::{Span, field, instrument};
use uuid::Uuid;
use watchkeep_catalog::{
    CANDIDATE_GROUP_GENRES, Candidate, Catalog, CatalogMovie, CatalogShow, GenreWeights, TasteFacts,
};
use watchkeep_storage::clock::{SharedClock, date_of};
use watchkeep_storage::model::MediaKind;
use watchkeep_storage::queries::{Queries, TasteItem};

// region: constants

/// The weight of one watched item before the signals change it.
const BASE_WEIGHT: f64 = 1.0;

/// Extra weight for every play after the first pass, up to `MAX_REPLAY_BONUS`.
const REPLAY_BONUS: f64 = 0.5;
const MAX_REPLAY_BONUS: f64 = 1.0;

/// The weight that a show keeps when almost none of it is watched. A show that
/// is complete keeps the full weight, so a show that was dropped counts less.
const MIN_COMPLETION_WEIGHT: f64 = 0.2;

/// The rating that means "as good as the rest", on the 0 to 10 scale of the
/// `ratings` table. A better rating lifts the weight, a worse one cuts it.
const NEUTRAL_RATING: f64 = 6.5;
const MIN_RATING_WEIGHT: f64 = 0.1;
const MAX_RATING_WEIGHT: f64 = 2.0;

const DAYS_PER_YEAR: f64 = 365.0;
const HALF: f64 = 0.5;

/// A play loses half of its weight after this time.
const RECENCY_HALF_LIFE_DAYS: f64 = 2.0 * DAYS_PER_YEAR;

/// The weight that an old play keeps, so that old favourites still count.
const MIN_RECENCY_WEIGHT: f64 = 0.3;

/// How much the language that you watch most lifts a candidate in that language.
const LANGUAGE_BONUS: f64 = 0.5;

/// The top of the TMDB rating scale. It turns the quality into a factor from 0 to 1.
const MAX_CATALOG_RATING: f64 = 10.0;

/// How many genres the reason line of one recommendation names. The candidate
/// query groups by the same count, so a SQL cap per group matches this list.
const MAX_REASON_GENRES: usize = CANDIDATE_GROUP_GENRES as usize;

/// The reason of a show that is complete and still makes episodes.
const RETURNING_REASON: &str = "New episodes in production";

/// How many items each list of the page holds.
pub const RECOMMENDATION_LIMIT: usize = 24;

/// How many items of one genre set a list of the page holds. The genre vector
/// of a profile with many watched items gives nearly the same affinity to every
/// candidate of the strongest genres, so the rating alone orders them and one
/// genre set takes the whole list. The cap leaves places for the sets below it.
const MAX_PER_GENRE_SET: usize = 3;

/// How many movies of one collection the collection list holds, so that a long
/// series does not fill it.
const MAX_PER_COLLECTION: usize = 2;

/// How many rows a candidate query returns. The query already keeps only
/// `MAX_PER_GENRE_SET` rows per genre group, so this cut no longer hides a
/// second pair behind one that would have filled the window.
pub const CANDIDATE_LIMIT: i64 = 200;

/// How many genres and how many languages the taste profile reports.
const PROFILE_TOP: usize = 5;

// endregion: constants

// region: response types

/// One item of a recommendation list.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation<T> {
    #[serde(flatten)]
    pub item: T,
    /// The library id, when the library already holds the item.
    pub local_id: Option<Uuid>,
    /// How well the item fits the profile, from 0.0 to 1.0.
    pub score: f64,
    /// Why the item is here: genre names, a collection name, or the production state.
    pub reasons: Vec<String>,
}

/// A part of the taste profile. `name` is a genre name, or an ISO 639-1
/// language code that the web UI turns into a language name.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TasteShare {
    pub name: String,
    /// Part of the profile weight, from 0.0 to 1.0.
    pub share: f64,
}

/// What the watch history says about taste. The page shows it, so that the
/// reason behind the lists is visible.
#[derive(Clone, Debug, Default, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TasteProfile {
    /// Watched items with a TMDB id. Nothing else feeds the profile.
    pub items: i64,
    pub genres: Vec<TasteShare>,
    pub languages: Vec<TasteShare>,
}

/// `GET /api/recommendations`. Every list is empty without a catalog.
#[derive(Clone, Debug, Default, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Recommendations {
    pub profile: TasteProfile,
    pub movies: Vec<Recommendation<CatalogMovie>>,
    pub shows: Vec<Recommendation<CatalogShow>>,
    /// Movies of a collection that the library already holds a part of.
    pub next_in_collection: Vec<Recommendation<CatalogMovie>>,
    /// Shows of the library that are complete and still make episodes.
    pub returning: Vec<Recommendation<CatalogShow>>,
}

// endregion: response types

// region: the profile

/// The weight of one watched item. An item with no play weighs nothing: it only
/// keeps its TMDB id out of the candidates.
fn weight_of(item: &TasteItem, total_episodes: i64, now: DateTime<Utc>) -> f64 {
    if item.watched_count == 0 {
        return 0.0;
    }
    let replays = (item.play_count - item.watched_count).max(0) as f64;
    let mut weight = BASE_WEIGHT * (1.0 + (replays * REPLAY_BONUS).min(MAX_REPLAY_BONUS));
    if item.kind == MediaKind::Show && total_episodes > 0 {
        let completion = (item.watched_count as f64 / total_episodes as f64).clamp(0.0, 1.0);
        weight *= MIN_COMPLETION_WEIGHT + (1.0 - MIN_COMPLETION_WEIGHT) * completion;
    }
    if let Some(rating) = item.rating {
        weight *= (rating / NEUTRAL_RATING).clamp(MIN_RATING_WEIGHT, MAX_RATING_WEIGHT);
    }
    weight * recency_of(item.last_watched_at, now)
}

/// How much of its weight a play keeps, by its age.
fn recency_of(last_watched_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let Some(last) = last_watched_at else {
        return MIN_RECENCY_WEIGHT;
    };
    let days = (now - last).num_days().max(0) as f64;
    HALF.powf(days / RECENCY_HALF_LIFE_DAYS)
        .max(MIN_RECENCY_WEIGHT)
}

/// A weight per key as a unit vector, so that a dot product against it is a cosine.
fn unit_vector(weights: &HashMap<i64, f64>) -> GenreWeights {
    let norm = weights
        .values()
        .map(|weight| weight * weight)
        .sum::<f64>()
        .sqrt();
    if norm <= 0.0 {
        return GenreWeights::default();
    }
    let mut ids: Vec<i64> = weights.keys().copied().collect();
    ids.sort_unstable();
    let values = ids.iter().map(|id| weights[id] / norm).collect();
    GenreWeights {
        ids,
        weights: values,
    }
}

/// The largest share first, as a share of the total, cut to `PROFILE_TOP`.
fn top_shares(weights: &HashMap<String, f64>) -> Vec<TasteShare> {
    let total: f64 = weights.values().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut shares: Vec<TasteShare> = weights
        .iter()
        .map(|(name, weight)| TasteShare {
            name: name.clone(),
            share: weight / total,
        })
        .collect();
    shares.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then_with(|| a.name.cmp(&b.name))
    });
    shares.truncate(PROFILE_TOP);
    shares
}

/// The watch history as the numbers that the catalog queries need.
#[derive(Default)]
struct Taste {
    /// Weight per movie genre id and per show genre id, before the unit vector.
    /// One name can appear in both maps, so neither map is the profile.
    movie_genres: HashMap<i64, f64>,
    show_genres: HashMap<i64, f64>,
    /// Weight per genre name. This is the profile: one entry per genre, whether
    /// a movie, a show, or both carry it.
    genre_weights: HashMap<String, f64>,
    /// Weight per collection of a watched movie.
    collections: HashMap<i64, f64>,
    /// Weight per ISO 639-1 language code.
    languages: HashMap<String, f64>,
    /// TMDB ids that the library already holds, per kind.
    movie_ids: Vec<i64>,
    show_ids: Vec<i64>,
    /// Library shows that are complete and still make episodes.
    returning: Vec<(Uuid, i64, f64)>,
    watched_items: i64,
    max_weight: f64,
}

impl Taste {
    /// Genre weights travel by genre name, because TMDB numbers the genres of a
    /// movie and of a show apart. A name that only one kind has stays with that
    /// kind, so "Sci-Fi & Fantasy" never reaches a movie.
    fn build(
        items: &[TasteItem],
        movie_facts: &HashMap<i64, TasteFacts>,
        show_facts: &HashMap<i64, TasteFacts>,
        genre_names: &GenreNames,
        episode_totals: &HashMap<i64, i64>,
        now: DateTime<Utc>,
    ) -> Self {
        let mut taste = Self::default();
        let mut by_genre_name: HashMap<String, f64> = HashMap::new();
        for item in items {
            let is_movie = item.kind == MediaKind::Movie;
            if is_movie {
                taste.movie_ids.push(item.tmdb_id);
            } else {
                taste.show_ids.push(item.tmdb_id);
            }
            let facts = if is_movie {
                movie_facts.get(&item.tmdb_id)
            } else {
                show_facts.get(&item.tmdb_id)
            };
            let known = facts.map(|facts| facts.genres.len()).unwrap_or(0);
            let total_episodes = episode_totals
                .get(&item.tmdb_id)
                .copied()
                .unwrap_or(0)
                .max(item.episode_count);
            let weight = weight_of(item, total_episodes, now);
            if weight <= 0.0 {
                continue;
            }
            taste.watched_items += 1;
            taste.max_weight = taste.max_weight.max(weight);
            let Some(facts) = facts else { continue };
            if let Some(language) = &facts.language {
                *taste.languages.entry(language.clone()).or_default() += weight;
            }
            if let Some(collection_id) = facts.collection_id {
                *taste.collections.entry(collection_id).or_default() += weight;
            }
            if !is_movie
                && facts.in_production
                && total_episodes > 0
                && item.watched_count >= total_episodes
            {
                taste.returning.push((item.id, item.tmdb_id, weight));
            }
            // A genre of an item shares the weight of the item, so that an item
            // with many genres does not count more than an item with one.
            let share = weight / known.max(1) as f64;
            for genre_id in &facts.genres {
                let names = if is_movie {
                    &genre_names.movies
                } else {
                    &genre_names.shows
                };
                if let Some(name) = names.get(genre_id) {
                    *by_genre_name.entry(name.clone()).or_default() += share;
                }
            }
        }
        for (genre_id, name) in &genre_names.movies {
            if let Some(weight) = by_genre_name.get(name) {
                taste.movie_genres.insert(*genre_id, *weight);
            }
        }
        for (genre_id, name) in &genre_names.shows {
            if let Some(weight) = by_genre_name.get(name) {
                taste.show_genres.insert(*genre_id, *weight);
            }
        }
        taste.genre_weights = by_genre_name;
        taste
    }

    fn collection_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.collections.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// The weights per name, not the two id maps: a genre that both a movie and
    /// a show carry holds one id in each map and must count only once.
    fn profile(&self) -> TasteProfile {
        TasteProfile {
            items: self.watched_items,
            genres: top_shares(&self.genre_weights),
            languages: top_shares(&self.languages),
        }
    }

    /// The share of the profile that one language holds, from 0.0 to 1.0.
    fn language_share(&self, language: Option<&String>) -> f64 {
        let total: f64 = self.languages.values().sum();
        match (language, total > 0.0) {
            (Some(language), true) => self.languages.get(language).copied().unwrap_or(0.0) / total,
            _ => 0.0,
        }
    }

    /// A weight as a part of the largest weight of the profile, from 0.0 to 1.0.
    fn relative(&self, weight: f64) -> f64 {
        if self.max_weight <= 0.0 {
            0.0
        } else {
            (weight / self.max_weight).clamp(0.0, 1.0)
        }
    }
}

/// The genre names of both kinds, for the reason lines and for the name bridge.
#[derive(Default)]
struct GenreNames {
    movies: HashMap<i64, String>,
    shows: HashMap<i64, String>,
}

// endregion: the profile

// region: the service

#[derive(Clone)]
pub struct Recommender {
    queries: Queries,
    catalog: Option<Catalog>,
    clock: SharedClock,
}

impl Recommender {
    pub fn new(queries: Queries, catalog: Option<Catalog>, clock: SharedClock) -> Self {
        Self {
            queries,
            catalog,
            clock,
        }
    }

    /// Without a catalog there is nothing to recommend, so every list is empty.
    // The titles of the library are the watch history of a person, so no title
    // goes on the span. The counts say enough to read the latency.
    #[instrument(
        skip_all,
        err,
        fields(taste.items = field::Empty, catalog.enabled = self.catalog.is_some())
    )]
    pub async fn recommendations(&self) -> Result<Recommendations> {
        let Some(catalog) = &self.catalog else {
            return Ok(Recommendations::default());
        };
        let now = self.clock.now();
        let today = date_of(now);
        let items = self.queries.taste().await?;
        let movie_ids: Vec<i64> = ids_of(&items, MediaKind::Movie);
        let show_ids: Vec<i64> = ids_of(&items, MediaKind::Show);
        let (movie_facts, show_facts, movie_names, show_names, episode_totals) = tokio::try_join!(
            catalog.movie_taste_facts(&movie_ids),
            catalog.show_taste_facts(&show_ids),
            catalog.movie_genre_names(),
            catalog.show_genre_names(),
            catalog.aired_episode_counts(&show_ids, today),
        )?;
        let genre_names = GenreNames {
            movies: movie_names,
            shows: show_names,
        };
        let taste = Taste::build(
            &items,
            &movie_facts,
            &show_facts,
            &genre_names,
            &episode_totals,
            now,
        );
        Span::current().record("taste.items", taste.watched_items);
        let movie_genres = unit_vector(&taste.movie_genres);
        let show_genres = unit_vector(&taste.show_genres);
        let collection_ids = taste.collection_ids();
        // Collection movies have their own list. Exclude them from the genre
        // query so a per-group cap there is not spent on a row that this list
        // will drop.
        let collection_movies = catalog
            .collection_movies(&collection_ids, &taste.movie_ids, today, CANDIDATE_LIMIT)
            .await?;
        let next_in_collection = self.collection_list(collection_movies, &taste);
        let mut movie_exclude = taste.movie_ids.clone();
        movie_exclude.extend(next_in_collection.iter().map(|item| item.item.tmdb_id));
        let (movies, shows) = tokio::try_join!(
            catalog.movie_candidates(
                &movie_genres,
                &movie_exclude,
                today,
                CANDIDATE_LIMIT,
                MAX_PER_GENRE_SET as i64,
            ),
            catalog.show_candidates(
                &show_genres,
                &taste.show_ids,
                today,
                CANDIDATE_LIMIT,
                MAX_PER_GENRE_SET as i64,
            ),
        )?;
        let returning = self.returning_list(catalog, &taste).await?;
        Ok(Recommendations {
            profile: taste.profile(),
            movies: rank(movies, &taste, &taste.movie_genres, &genre_names.movies),
            shows: rank(shows, &taste, &taste.show_genres, &genre_names.shows),
            next_in_collection,
            returning,
        })
    }

    /// The missing parts of a collection, best first. The weight of the
    /// collection in the profile replaces the genre affinity.
    fn collection_list(
        &self,
        candidates: Vec<watchkeep_catalog::CollectionMovie>,
        taste: &Taste,
    ) -> Vec<Recommendation<CatalogMovie>> {
        let items: Vec<(i64, Recommendation<CatalogMovie>)> = candidates
            .into_iter()
            .map(|candidate| {
                let weight = taste
                    .collections
                    .get(&candidate.collection_id)
                    .copied()
                    .unwrap_or(0.0);
                (
                    candidate.collection_id,
                    Recommendation {
                        local_id: None,
                        score: taste.relative(weight) * candidate.quality / MAX_CATALOG_RATING,
                        reasons: candidate.collection_name.into_iter().collect(),
                        item: candidate.movie,
                    },
                )
            })
            .collect();
        take_diverse(items, MAX_PER_COLLECTION)
    }

    /// Shows of the library that are complete and still make episodes. The
    /// catalog supplies the poster and the year, the library supplies the id.
    async fn returning_list(
        &self,
        catalog: &Catalog,
        taste: &Taste,
    ) -> Result<Vec<Recommendation<CatalogShow>>> {
        let ids: Vec<i64> = taste
            .returning
            .iter()
            .map(|(_, tmdb_id, _)| *tmdb_id)
            .collect();
        let shows = catalog.shows_by_tmdb_ids(&ids).await?;
        let mut list: Vec<Recommendation<CatalogShow>> = taste
            .returning
            .iter()
            .filter_map(|(id, tmdb_id, weight)| {
                Some(Recommendation {
                    item: shows.get(tmdb_id)?.clone(),
                    local_id: Some(*id),
                    score: taste.relative(*weight),
                    reasons: vec![RETURNING_REASON.to_owned()],
                })
            })
            .collect();
        sort_by_score(&mut list);
        Ok(list)
    }
}

fn ids_of(items: &[TasteItem], kind: MediaKind) -> Vec<i64> {
    items
        .iter()
        .filter(|item| item.kind == kind)
        .map(|item| item.tmdb_id)
        .collect()
}

/// The final score of a genre candidate: how well it fits, how good it is, and
/// a lift for a language that the history is full of. `genre_weights` and
/// `genre_names` belong to the same kind as the candidates, because one TMDB
/// genre id can name a movie genre and a show genre at the same time.
fn rank<T>(
    candidates: Vec<Candidate<T>>,
    taste: &Taste,
    genre_weights: &HashMap<i64, f64>,
    genre_names: &HashMap<i64, String>,
) -> Vec<Recommendation<T>> {
    let weights = |genre_id: &i64| -> f64 { genre_weights.get(genre_id).copied().unwrap_or(0.0) };
    let items: Vec<(Vec<i64>, Recommendation<T>)> = candidates
        .into_iter()
        .map(|candidate| {
            let language = taste.language_share(candidate.language.as_ref());
            let score = candidate.affinity
                * (candidate.quality / MAX_CATALOG_RATING)
                * (1.0 + LANGUAGE_BONUS * language);
            // A genre that the profile does not hold is no reason for the pick.
            let mut genres = candidate.genres;
            genres.retain(|genre_id| weights(genre_id) > 0.0);
            genres.sort_by(|a, b| weights(b).total_cmp(&weights(a)).then_with(|| a.cmp(b)));
            genres.truncate(MAX_REASON_GENRES);
            let reasons = genres
                .iter()
                .filter_map(|genre_id| genre_names.get(genre_id).cloned())
                .collect();
            (
                genres,
                Recommendation {
                    item: candidate.item,
                    local_id: None,
                    score: score.clamp(0.0, 1.0),
                    reasons,
                },
            )
        })
        .collect();
    take_diverse(items, MAX_PER_GENRE_SET)
}

/// The best items, best first, with at most `cap` items per group. A group is
/// the genre set that the reason line names, or the collection of a movie.
/// Every item of one group scores almost the same, so without the cap the best
/// group fills the list and nothing else appears. The list is then shorter than
/// `RECOMMENDATION_LIMIT` when the candidates hold few groups: a short list of
/// different things says more than a long list of one thing.
fn take_diverse<T, K: Eq + Hash>(
    mut items: Vec<(K, Recommendation<T>)>,
    cap: usize,
) -> Vec<Recommendation<T>> {
    items.sort_by(|(_, a), (_, b)| b.score.total_cmp(&a.score));
    let mut taken: HashMap<K, usize> = HashMap::new();
    let mut list = Vec::new();
    for (group, item) in items {
        if list.len() >= RECOMMENDATION_LIMIT {
            break;
        }
        let count = taken.entry(group).or_default();
        if *count < cap {
            *count += 1;
            list.push(item);
        }
    }
    list
}

fn sort_by_score<T>(items: &mut Vec<Recommendation<T>>) {
    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    items.truncate(RECOMMENDATION_LIMIT);
}

// endregion: the service
