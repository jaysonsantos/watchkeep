//! Set-based writes for imports. Each function runs a handful of queries for
//! any number of rows, so imports stay fast over a slow network link. Rows
//! travel as arrays, and `UNNEST` turns them into an insert.
//! The matching rules mirror `Library::find_media` and `Library::upsert_episode`.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use eyre::Result;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::model::{
    EpisodeInput, EpisodeRow, ExternalIds, MediaKind, MediaRow, MovieRef, PlaySource, RatingKind,
    ShowRef, TargetKind, millis, new_id, non_empty, tmdb_number,
};

/// A movie or show input for the bulk upsert.
#[derive(Clone, Copy)]
pub enum MediaInput<'a> {
    Movie(&'a MovieRef),
    Show(&'a ShowRef),
}

impl<'a> MediaInput<'a> {
    fn title(&self) -> &'a str {
        match self {
            MediaInput::Movie(movie) => &movie.title,
            MediaInput::Show(show) => &show.title,
        }
    }

    fn year(&self) -> Option<i32> {
        match self {
            MediaInput::Movie(movie) => movie.year,
            MediaInput::Show(show) => show.year,
        }
    }

    fn ids(&self) -> &'a ExternalIds {
        match self {
            MediaInput::Movie(movie) => &movie.ids,
            MediaInput::Show(show) => &show.ids,
        }
    }

    fn duration(&self) -> Option<Duration> {
        match self {
            MediaInput::Movie(movie) => movie.duration,
            MediaInput::Show(_) => None,
        }
    }

    fn summary(&self) -> Option<&'a str> {
        match self {
            MediaInput::Movie(movie) => movie.summary.as_deref(),
            MediaInput::Show(show) => show.summary.as_deref(),
        }
    }

    fn poster_path(&self) -> Option<&'a str> {
        match self {
            MediaInput::Movie(movie) => movie.poster_path.as_deref(),
            MediaInput::Show(show) => show.poster_path.as_deref(),
        }
    }
}

/// The unique id columns. Each value can belong to one row only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum IdColumn {
    PlexGuid,
    Tmdb,
    Tvdb,
    Imdb,
}

/// The row that owns an id inside one batch: an existing row or a queued insert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Row(Uuid),
    New(usize),
}

/// Tracks which row owns each unique id inside one batch, so that two inputs
/// that carry the same id never produce two rows with it.
#[derive(Default)]
struct Claims {
    owners: HashMap<(IdColumn, String), Owner>,
}

impl Claims {
    /// Returns the value when `owner` may use it, or `None` when another row already has it.
    fn claim<T: ToString>(
        &mut self,
        column: IdColumn,
        value: Option<T>,
        owner: Owner,
    ) -> Option<T> {
        let value = value?;
        let key = (column, value.to_string());
        match self.owners.get(&key) {
            Some(current) if *current != owner => None,
            _ => {
                self.owners.insert(key, owner);
                Some(value)
            }
        }
    }
}

trait MediaKeys {
    fn plex_guid(&self) -> Option<&str>;
    fn tmdb_id(&self) -> Option<i64>;
    fn tvdb_id(&self) -> Option<&str>;
    fn imdb_id(&self) -> Option<&str>;
    fn title(&self) -> &str;
    fn year(&self) -> Option<i32>;
}

impl MediaKeys for MediaRow {
    fn plex_guid(&self) -> Option<&str> {
        self.plex_guid.as_deref()
    }
    fn tmdb_id(&self) -> Option<i64> {
        self.tmdb_id
    }
    fn tvdb_id(&self) -> Option<&str> {
        self.tvdb_id.as_deref()
    }
    fn imdb_id(&self) -> Option<&str> {
        self.imdb_id.as_deref()
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn year(&self) -> Option<i32> {
        self.year
    }
}

/// Finds rows by any id and then by title and year, the same way `Library::find_media` does.
struct MediaIndex<T> {
    items: Vec<T>,
    by_guid: HashMap<String, usize>,
    by_tmdb: HashMap<i64, usize>,
    by_tvdb: HashMap<String, usize>,
    by_imdb: HashMap<String, usize>,
    by_title: HashMap<String, Vec<usize>>,
}

impl<T: MediaKeys> MediaIndex<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            by_guid: HashMap::new(),
            by_tmdb: HashMap::new(),
            by_tvdb: HashMap::new(),
            by_imdb: HashMap::new(),
            by_title: HashMap::new(),
        }
    }

    fn add(&mut self, item: T) -> usize {
        let index = self.items.len();
        if let Some(guid) = non_empty(item.plex_guid()) {
            self.by_guid.insert(guid.to_owned(), index);
        }
        if let Some(tmdb) = item.tmdb_id() {
            self.by_tmdb.insert(tmdb, index);
        }
        if let Some(tvdb) = non_empty(item.tvdb_id()) {
            self.by_tvdb.insert(tvdb.to_owned(), index);
        }
        if let Some(imdb) = non_empty(item.imdb_id()) {
            self.by_imdb.insert(imdb.to_owned(), index);
        }
        self.by_title
            .entry(item.title().to_lowercase())
            .or_default()
            .push(index);
        self.items.push(item);
        index
    }

    fn find(&self, input: &MediaInput<'_>) -> Option<usize> {
        let ids = input.ids();
        let found = non_empty(ids.plex_guid.as_deref())
            .and_then(|guid| self.by_guid.get(guid))
            .or_else(|| tmdb_number(ids.tmdb.as_deref()).and_then(|tmdb| self.by_tmdb.get(&tmdb)))
            .or_else(|| non_empty(ids.tvdb.as_deref()).and_then(|tvdb| self.by_tvdb.get(tvdb)))
            .or_else(|| non_empty(ids.imdb.as_deref()).and_then(|imdb| self.by_imdb.get(imdb)));
        if let Some(index) = found {
            return Some(*index);
        }
        let candidates = self.by_title.get(&input.title().to_lowercase())?;
        candidates
            .iter()
            .find(|index| self.items[**index].year() == input.year())
            .or_else(|| {
                candidates
                    .iter()
                    .find(|index| self.items[**index].year().is_none() || input.year().is_none())
            })
            .copied()
    }
}

struct PendingMedia<'a> {
    input: MediaInput<'a>,
    positions: Vec<usize>,
    title: String,
    year: Option<i32>,
    plex_guid: Option<String>,
    tmdb_id: Option<i64>,
    tvdb_id: Option<String>,
    imdb_id: Option<String>,
}

impl MediaKeys for PendingMedia<'_> {
    fn plex_guid(&self) -> Option<&str> {
        self.plex_guid.as_deref()
    }
    fn tmdb_id(&self) -> Option<i64> {
        self.tmdb_id
    }
    fn tvdb_id(&self) -> Option<&str> {
        self.tvdb_id.as_deref()
    }
    fn imdb_id(&self) -> Option<&str> {
        self.imdb_id.as_deref()
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn year(&self) -> Option<i32> {
        self.year
    }
}

/// Insert or update many movies or shows. Returns one row per input, in input
/// order. Inputs that resolve to the same row share it.
pub async fn bulk_upsert_media(
    conn: &mut PgConnection,
    kind: MediaKind,
    inputs: &[MediaInput<'_>],
    now: DateTime<Utc>,
) -> Result<Vec<MediaRow>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let guids: Vec<String> = inputs
        .iter()
        .filter_map(|input| input.ids().plex_guid.clone())
        .collect();
    let tmdbs: Vec<i64> = inputs
        .iter()
        .filter_map(|input| tmdb_number(input.ids().tmdb.as_deref()))
        .collect();
    let tvdbs: Vec<String> = inputs
        .iter()
        .filter_map(|input| input.ids().tvdb.clone())
        .collect();
    let imdbs: Vec<String> = inputs
        .iter()
        .filter_map(|input| input.ids().imdb.clone())
        .collect();
    let titles: Vec<String> = inputs
        .iter()
        .map(|input| input.title().to_lowercase())
        .collect();
    let existing = sqlx::query_as!(
        MediaRow,
        r#"SELECT id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                  duration_ms, summary, poster_path, hidden_at, created_at, updated_at
           FROM media
           WHERE kind = $1 AND (plex_guid = ANY($2) OR tmdb_id = ANY($3) OR tvdb_id = ANY($4) OR imdb_id = ANY($5) OR lower(title) = ANY($6))"#,
        kind.as_str(),
        &guids,
        &tmdbs,
        &tvdbs,
        &imdbs,
        &titles
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut claims = Claims::default();
    let mut index = MediaIndex::new();
    for row in existing {
        let owner = Owner::Row(row.id);
        claims.claim(IdColumn::PlexGuid, row.plex_guid.clone(), owner);
        claims.claim(IdColumn::Tmdb, row.tmdb_id, owner);
        claims.claim(IdColumn::Tvdb, row.tvdb_id.clone(), owner);
        claims.claim(IdColumn::Imdb, row.imdb_id.clone(), owner);
        index.add(row);
    }

    // Pass 1: match inputs to existing rows and collect merged updates.
    let mut placements: Vec<Option<Owner>> = vec![None; inputs.len()];
    let mut updates: HashMap<Uuid, MediaRow> = HashMap::new();
    let mut pending_insert: Vec<(usize, MediaInput<'_>)> = Vec::new();
    for (position, input) in inputs.iter().enumerate() {
        let Some(found) = index.find(input) else {
            pending_insert.push((position, *input));
            continue;
        };
        let current = &index.items[found];
        let owner = Owner::Row(current.id);
        let base = updates.get(&current.id).unwrap_or(current);
        let ids = input.ids();
        let merged = MediaRow {
            title: input.title().to_owned(),
            year: input.year().or(base.year),
            plex_guid: claims
                .claim(IdColumn::PlexGuid, ids.plex_guid.clone(), owner)
                .or_else(|| base.plex_guid.clone()),
            imdb_id: claims
                .claim(IdColumn::Imdb, ids.imdb.clone(), owner)
                .or_else(|| base.imdb_id.clone()),
            tmdb_id: claims
                .claim(IdColumn::Tmdb, tmdb_number(ids.tmdb.as_deref()), owner)
                .or(base.tmdb_id),
            tvdb_id: claims
                .claim(IdColumn::Tvdb, ids.tvdb.clone(), owner)
                .or_else(|| base.tvdb_id.clone()),
            duration_ms: millis(input.duration()).or(base.duration_ms),
            summary: input
                .summary()
                .map(str::to_owned)
                .or_else(|| base.summary.clone()),
            poster_path: input
                .poster_path()
                .map(str::to_owned)
                .or_else(|| base.poster_path.clone()),
            updated_at: now,
            ..base.clone()
        };
        updates.insert(current.id, merged);
        placements[position] = Some(owner);
    }

    // Pass 2: insert new rows. Inputs that match a queued row by any id share it.
    let mut pending_index: MediaIndex<PendingMedia<'_>> = MediaIndex::new();
    for (position, input) in pending_insert {
        if let Some(queued) = pending_index.find(&input) {
            pending_index.items[queued].positions.push(position);
            continue;
        }
        let slot = pending_index.items.len();
        let owner = Owner::New(slot);
        let ids = input.ids();
        let pending = PendingMedia {
            title: input.title().to_owned(),
            year: input.year(),
            plex_guid: claims.claim(IdColumn::PlexGuid, ids.plex_guid.clone(), owner),
            tmdb_id: claims.claim(IdColumn::Tmdb, tmdb_number(ids.tmdb.as_deref()), owner),
            tvdb_id: claims.claim(IdColumn::Tvdb, ids.tvdb.clone(), owner),
            imdb_id: claims.claim(IdColumn::Imdb, ids.imdb.clone(), owner),
            positions: vec![position],
            input,
        };
        pending_index.add(pending);
    }
    let queued = &pending_index.items;
    let created = if queued.is_empty() {
        Vec::new()
    } else {
        let ids: Vec<Uuid> = queued.iter().map(|_| new_id(now)).collect();
        let titles: Vec<String> = queued.iter().map(|pending| pending.title.clone()).collect();
        let years: Vec<Option<i32>> = queued.iter().map(|pending| pending.year).collect();
        let plex_guids: Vec<Option<String>> = queued
            .iter()
            .map(|pending| pending.plex_guid.clone())
            .collect();
        let imdb_ids: Vec<Option<String>> = queued
            .iter()
            .map(|pending| pending.imdb_id.clone())
            .collect();
        let tmdb_ids: Vec<Option<i64>> = queued.iter().map(|pending| pending.tmdb_id).collect();
        let tvdb_ids: Vec<Option<String>> = queued
            .iter()
            .map(|pending| pending.tvdb_id.clone())
            .collect();
        let durations: Vec<Option<i64>> = queued
            .iter()
            .map(|pending| millis(pending.input.duration()))
            .collect();
        let summaries: Vec<Option<String>> = queued
            .iter()
            .map(|pending| pending.input.summary().map(str::to_owned))
            .collect();
        let posters: Vec<Option<String>> = queued
            .iter()
            .map(|pending| pending.input.poster_path().map(str::to_owned))
            .collect();
        sqlx::query_as!(
            MediaRow,
            r#"INSERT INTO media (id, kind, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, created_at, updated_at)
               SELECT u.id, $2::text, u.title, u.year, u.plex_guid, u.imdb_id, u.tmdb_id, u.tvdb_id, u.duration_ms, u.summary, u.poster_path, $12::timestamptz, $12::timestamptz
               FROM UNNEST($1::uuid[], $3::text[], $4::int[], $5::text[], $6::text[], $7::bigint[], $8::text[], $9::bigint[], $10::text[], $11::text[])
                 WITH ORDINALITY AS u(id, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, ord)
               ORDER BY u.ord
               RETURNING id, kind AS "kind: MediaKind", title, year, plex_guid, imdb_id, tmdb_id, tvdb_id,
                         duration_ms, summary, poster_path, hidden_at, created_at, updated_at"#,
            &ids,
            kind.as_str(),
            &titles,
            &years as &[Option<i32>],
            &plex_guids as &[Option<String>],
            &imdb_ids as &[Option<String>],
            &tmdb_ids as &[Option<i64>],
            &tvdb_ids as &[Option<String>],
            &durations as &[Option<i64>],
            &summaries as &[Option<String>],
            &posters as &[Option<String>],
            now
        )
        .fetch_all(&mut *conn)
        .await?
    };
    for (slot, pending) in queued.iter().enumerate() {
        for position in &pending.positions {
            placements[*position] = Some(Owner::New(slot));
        }
    }

    // Pass 3: apply the merged updates in one statement.
    if !updates.is_empty() {
        let rows: Vec<&MediaRow> = updates.values().collect();
        sqlx::query!(
            "UPDATE media AS m SET title = u.title, year = u.year, plex_guid = u.plex_guid, imdb_id = u.imdb_id, tmdb_id = u.tmdb_id,
               tvdb_id = u.tvdb_id, duration_ms = u.duration_ms, summary = u.summary, poster_path = u.poster_path, updated_at = u.updated_at
             FROM UNNEST($1::uuid[], $2::text[], $3::int[], $4::text[], $5::text[], $6::bigint[], $7::text[], $8::bigint[], $9::text[], $10::text[], $11::timestamptz[])
               AS u(id, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, updated_at)
             WHERE m.id = u.id",
            &rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            &rows.iter().map(|row| row.title.clone()).collect::<Vec<_>>(),
            &rows.iter().map(|row| row.year).collect::<Vec<_>>() as &[Option<i32>],
            &rows.iter().map(|row| row.plex_guid.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.imdb_id.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.tmdb_id).collect::<Vec<_>>() as &[Option<i64>],
            &rows.iter().map(|row| row.tvdb_id.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.duration_ms).collect::<Vec<_>>() as &[Option<i64>],
            &rows.iter().map(|row| row.summary.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.poster_path.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.updated_at).collect::<Vec<_>>()
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(placements
        .into_iter()
        .map(
            |placement| match placement.expect("every input has a placement") {
                Owner::Row(id) => updates[&id].clone(),
                Owner::New(slot) => created[slot].clone(),
            },
        )
        .collect())
}

pub struct EpisodeUpsert<'a> {
    pub show_id: Uuid,
    pub input: &'a EpisodeInput,
}

struct PendingEpisode<'a> {
    upsert: &'a EpisodeUpsert<'a>,
    plex_guid: Option<String>,
    tmdb_id: Option<i64>,
    positions: Vec<usize>,
}

/// Insert or update many episodes. Returns one row per input, in input order.
pub async fn bulk_upsert_episodes(
    conn: &mut PgConnection,
    inputs: &[EpisodeUpsert<'_>],
    now: DateTime<Utc>,
) -> Result<Vec<EpisodeRow>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let show_ids: Vec<Uuid> = inputs.iter().map(|input| input.show_id).collect();
    let guids: Vec<String> = inputs
        .iter()
        .filter_map(|input| input.input.ids.plex_guid.clone())
        .collect();
    let existing = sqlx::query_as!(
        EpisodeRow,
        "SELECT id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at
         FROM episodes WHERE show_id = ANY($1) OR plex_guid = ANY($2)",
        &show_ids,
        &guids
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut by_guid: HashMap<String, usize> = HashMap::new();
    let mut by_number: HashMap<(Uuid, i32, i32), usize> = HashMap::new();
    let mut claims = Claims::default();
    for (index, row) in existing.iter().enumerate() {
        if let Some(guid) = non_empty(row.plex_guid.as_deref()) {
            by_guid.insert(guid.to_owned(), index);
        }
        by_number.insert((row.show_id, row.season, row.number), index);
        let owner = Owner::Row(row.id);
        claims.claim(IdColumn::PlexGuid, row.plex_guid.clone(), owner);
        claims.claim(IdColumn::Tmdb, row.tmdb_id, owner);
    }

    let mut placements: Vec<Option<Owner>> = vec![None; inputs.len()];
    let mut updates: HashMap<Uuid, EpisodeRow> = HashMap::new();
    let mut to_insert: Vec<PendingEpisode<'_>> = Vec::new();
    let mut insert_by_number: HashMap<(Uuid, i32, i32), usize> = HashMap::new();
    let mut insert_by_guid: HashMap<String, usize> = HashMap::new();
    for (position, upsert) in inputs.iter().enumerate() {
        let input = upsert.input;
        let number_key = (upsert.show_id, input.season, input.number);
        let current = non_empty(input.ids.plex_guid.as_deref())
            .and_then(|guid| by_guid.get(guid))
            .or_else(|| by_number.get(&number_key))
            .map(|index| &existing[*index]);
        if let Some(current) = current {
            let owner = Owner::Row(current.id);
            let base = updates.get(&current.id).unwrap_or(current);
            let merged = EpisodeRow {
                show_id: upsert.show_id,
                season: input.season,
                number: input.number,
                title: input.title.clone().or_else(|| base.title.clone()),
                plex_guid: claims
                    .claim(IdColumn::PlexGuid, input.ids.plex_guid.clone(), owner)
                    .or_else(|| base.plex_guid.clone()),
                imdb_id: input.ids.imdb.clone().or_else(|| base.imdb_id.clone()),
                tmdb_id: claims
                    .claim(
                        IdColumn::Tmdb,
                        tmdb_number(input.ids.tmdb.as_deref()),
                        owner,
                    )
                    .or(base.tmdb_id),
                tvdb_id: input.ids.tvdb.clone().or_else(|| base.tvdb_id.clone()),
                duration_ms: millis(input.duration).or(base.duration_ms),
                aired_at: input.aired_at.or(base.aired_at),
                updated_at: now,
                ..base.clone()
            };
            updates.insert(current.id, merged);
            placements[position] = Some(owner);
            continue;
        }
        let guid = non_empty(input.ids.plex_guid.as_deref());
        let slot = insert_by_number
            .get(&number_key)
            .or_else(|| guid.and_then(|guid| insert_by_guid.get(guid)))
            .copied();
        if let Some(slot) = slot {
            to_insert[slot].positions.push(position);
            continue;
        }
        let slot = to_insert.len();
        let owner = Owner::New(slot);
        insert_by_number.insert(number_key, slot);
        if let Some(guid) = guid {
            insert_by_guid.insert(guid.to_owned(), slot);
        }
        to_insert.push(PendingEpisode {
            upsert,
            plex_guid: claims.claim(IdColumn::PlexGuid, input.ids.plex_guid.clone(), owner),
            tmdb_id: claims.claim(
                IdColumn::Tmdb,
                tmdb_number(input.ids.tmdb.as_deref()),
                owner,
            ),
            positions: vec![position],
        });
    }

    let created = if to_insert.is_empty() {
        Vec::new()
    } else {
        let ids: Vec<Uuid> = to_insert.iter().map(|_| new_id(now)).collect();
        let show_ids: Vec<Uuid> = to_insert
            .iter()
            .map(|pending| pending.upsert.show_id)
            .collect();
        let seasons: Vec<i32> = to_insert
            .iter()
            .map(|pending| pending.upsert.input.season)
            .collect();
        let numbers: Vec<i32> = to_insert
            .iter()
            .map(|pending| pending.upsert.input.number)
            .collect();
        let titles: Vec<Option<String>> = to_insert
            .iter()
            .map(|pending| pending.upsert.input.title.clone())
            .collect();
        let plex_guids: Vec<Option<String>> = to_insert
            .iter()
            .map(|pending| pending.plex_guid.clone())
            .collect();
        let imdb_ids: Vec<Option<String>> = to_insert
            .iter()
            .map(|pending| pending.upsert.input.ids.imdb.clone())
            .collect();
        let tmdb_ids: Vec<Option<i64>> = to_insert.iter().map(|pending| pending.tmdb_id).collect();
        let tvdb_ids: Vec<Option<String>> = to_insert
            .iter()
            .map(|pending| pending.upsert.input.ids.tvdb.clone())
            .collect();
        let durations: Vec<Option<i64>> = to_insert
            .iter()
            .map(|pending| millis(pending.upsert.input.duration))
            .collect();
        let aired: Vec<Option<NaiveDate>> = to_insert
            .iter()
            .map(|pending| pending.upsert.input.aired_at)
            .collect();
        sqlx::query_as!(
            EpisodeRow,
            "INSERT INTO episodes (id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at)
             SELECT u.id, u.show_id, u.season, u.number, u.title, u.plex_guid, u.imdb_id, u.tmdb_id, u.tvdb_id, u.duration_ms, u.aired_at, $12::timestamptz, $12::timestamptz
             FROM UNNEST($1::uuid[], $2::uuid[], $3::int[], $4::int[], $5::text[], $6::text[], $7::text[], $8::bigint[], $9::text[], $10::bigint[], $11::date[])
               WITH ORDINALITY AS u(id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, ord)
             ORDER BY u.ord
             RETURNING id, show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at",
            &ids,
            &show_ids,
            &seasons,
            &numbers,
            &titles as &[Option<String>],
            &plex_guids as &[Option<String>],
            &imdb_ids as &[Option<String>],
            &tmdb_ids as &[Option<i64>],
            &tvdb_ids as &[Option<String>],
            &durations as &[Option<i64>],
            &aired as &[Option<NaiveDate>],
            now
        )
        .fetch_all(&mut *conn)
        .await?
    };
    for (slot, pending) in to_insert.iter().enumerate() {
        for position in &pending.positions {
            placements[*position] = Some(Owner::New(slot));
        }
    }

    if !updates.is_empty() {
        let rows: Vec<&EpisodeRow> = updates.values().collect();
        sqlx::query!(
            "UPDATE episodes AS e SET title = u.title, plex_guid = u.plex_guid, imdb_id = u.imdb_id, tmdb_id = u.tmdb_id, tvdb_id = u.tvdb_id,
               duration_ms = u.duration_ms, aired_at = u.aired_at, updated_at = u.updated_at
             FROM UNNEST($1::uuid[], $2::text[], $3::text[], $4::text[], $5::bigint[], $6::text[], $7::bigint[], $8::date[], $9::timestamptz[])
               AS u(id, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, updated_at)
             WHERE e.id = u.id",
            &rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            &rows.iter().map(|row| row.title.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.plex_guid.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.imdb_id.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.tmdb_id).collect::<Vec<_>>() as &[Option<i64>],
            &rows.iter().map(|row| row.tvdb_id.clone()).collect::<Vec<_>>() as &[Option<String>],
            &rows.iter().map(|row| row.duration_ms).collect::<Vec<_>>() as &[Option<i64>],
            &rows.iter().map(|row| row.aired_at).collect::<Vec<_>>() as &[Option<NaiveDate>],
            &rows.iter().map(|row| row.updated_at).collect::<Vec<_>>()
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(placements
        .into_iter()
        .map(
            |placement| match placement.expect("every input has a placement") {
                Owner::Row(id) => updates[&id].clone(),
                Owner::New(slot) => created[slot].clone(),
            },
        )
        .collect())
}

pub struct PlayInsert {
    pub kind: TargetKind,
    pub id: Uuid,
    pub watched_at: DateTime<Utc>,
    pub source: PlaySource,
    pub external_id: Option<String>,
    pub account: Option<String>,
    pub player: Option<String>,
}

/// Insert plays; rows whose source and external id already exist are skipped. Returns the inserted count.
pub async fn bulk_record_plays(
    conn: &mut PgConnection,
    plays: &[PlayInsert],
    now: DateTime<Utc>,
) -> Result<u64> {
    if plays.is_empty() {
        return Ok(0);
    }
    let ids: Vec<Uuid> = plays.iter().map(|_| new_id(now)).collect();
    let kinds: Vec<String> = plays
        .iter()
        .map(|play| play.kind.as_str().to_owned())
        .collect();
    let targets: Vec<Uuid> = plays.iter().map(|play| play.id).collect();
    let watched: Vec<DateTime<Utc>> = plays.iter().map(|play| play.watched_at).collect();
    let sources: Vec<String> = plays
        .iter()
        .map(|play| play.source.as_str().to_owned())
        .collect();
    let accounts: Vec<Option<String>> = plays.iter().map(|play| play.account.clone()).collect();
    let players: Vec<Option<String>> = plays.iter().map(|play| play.player.clone()).collect();
    let external_ids: Vec<Option<String>> =
        plays.iter().map(|play| play.external_id.clone()).collect();
    Ok(sqlx::query!(
        "INSERT INTO plays (id, target_kind, target_id, watched_at, source, account, player, external_id)
         SELECT * FROM UNNEST($1::uuid[], $2::text[], $3::uuid[], $4::timestamptz[], $5::text[], $6::text[], $7::text[], $8::text[])
         ON CONFLICT (source, external_id) WHERE external_id IS NOT NULL DO NOTHING",
        &ids,
        &kinds,
        &targets,
        &watched,
        &sources,
        &accounts as &[Option<String>],
        &players as &[Option<String>],
        &external_ids as &[Option<String>]
    )
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

pub struct RatingInsert {
    pub kind: RatingKind,
    pub id: Uuid,
    pub rating: f64,
    pub rated_at: DateTime<Utc>,
}

/// Insert or update ratings. Each `(kind, id)` pair must appear once.
pub async fn bulk_set_ratings(conn: &mut PgConnection, ratings: &[RatingInsert]) -> Result<()> {
    if ratings.is_empty() {
        return Ok(());
    }
    let kinds: Vec<String> = ratings
        .iter()
        .map(|rating| rating.kind.as_str().to_owned())
        .collect();
    let ids: Vec<Uuid> = ratings.iter().map(|rating| rating.id).collect();
    let values: Vec<f64> = ratings.iter().map(|rating| rating.rating).collect();
    let rated: Vec<DateTime<Utc>> = ratings.iter().map(|rating| rating.rated_at).collect();
    sqlx::query!(
        "INSERT INTO ratings (target_kind, target_id, rating, rated_at)
         SELECT * FROM UNNEST($1::text[], $2::uuid[], $3::float8[], $4::timestamptz[])
         ON CONFLICT (target_kind, target_id) DO UPDATE SET rating = EXCLUDED.rating, rated_at = EXCLUDED.rated_at",
        &kinds,
        &ids,
        &values,
        &rated
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
