//! The statements of `sqlx` as spans.
//!
//! sqlx reports a statement as a `tracing` event on the target [`TARGET`],
//! never as a span, so a trace shows no database call. [`layer`] makes a layer
//! that reads that event and builds one `client` span from it. The event
//! carries the elapsed time, so the span starts that much before the event and
//! ends at the event.
//!
//! The layer carries its own filter, because the statement event is a `DEBUG`
//! event and the `RUST_LOG` filter of the other layers drops it.
//!
//! The layer touches no call site. Every `sqlx` query of the process gets a
//! span, in this crate and in any other crate that uses `sqlx` with
//! `tracing-opentelemetry`.
//!
//! The span follows the database conventions of OpenTelemetry: the name is
//! `db.query.summary`, the kind is `client`, and the attribute names come from
//! `opentelemetry-semantic-conventions`, never from a string in this file. See
//! <https://opentelemetry.io/docs/specs/semconv/database/database-spans/>.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use opentelemetry::trace::{Span as _, SpanBuilder, SpanKind, Tracer};
use opentelemetry::{KeyValue, Value};
use opentelemetry_semantic_conventions::attribute::{
    DB_COLLECTION_NAME, DB_NAMESPACE, DB_OPERATION_NAME, DB_QUERY_SUMMARY, DB_QUERY_TEXT,
    DB_RESPONSE_RETURNED_ROWS, DB_SYSTEM_NAME, SERVER_ADDRESS, SERVER_PORT,
};
use tracing::dispatcher::WeakDispatch;
use tracing::field::{Field, Visit};
use tracing::{Dispatch, Event, Metadata, Subscriber};
use tracing_opentelemetry::get_otel_context;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Filter};
use tracing_subscriber::registry::LookupSpan;

/// The target of every statement event of sqlx.
pub const TARGET: &str = "sqlx::query";

/// The fields of the statement event. sqlx writes them in `QueryLogger::finish`.
mod fields {
    /// The first words of the statement, with an ellipsis when the statement is
    /// longer. It is the whole statement when the statement is short.
    pub const SUMMARY: &str = "summary";
    /// The whole statement. sqlx leaves it empty when the summary already holds
    /// the whole statement.
    pub const STATEMENT: &str = "db.statement";
    pub const ROWS_RETURNED: &str = "rows_returned";
    pub const ROWS_AFFECTED: &str = "rows_affected";
    /// The duration of the statement in seconds, as a number.
    pub const ELAPSED_SECS: &str = "elapsed_secs";

    /// sqlx appends this to a summary that it cut.
    pub const ELLIPSIS: &str = " …";
}

/// The conventions have no name for the rows that a write changed, so this one
/// carries the crate name, not the `db` prefix of a standard attribute.
const SQLX_ROWS_AFFECTED: &str = "sqlx.rows_affected";

/// The `db.system.name` of each database that sqlx drives.
pub mod systems {
    pub const POSTGRESQL: &str = "postgresql";
    pub const MYSQL: &str = "mysql";
    pub const MARIADB: &str = "mariadb";
    pub const SQLITE: &str = "sqlite";
}

/// The port that PostgreSQL listens on without a setting.
const DEFAULT_POSTGRESQL_PORT: u16 = 5432;

/// The port that MySQL and MariaDB listen on without a setting.
const DEFAULT_MYSQL_PORT: u16 = 3306;

/// The name of a span whose statement gives no operation.
const UNNAMED_STATEMENT: &str = "query";

// region: configuration

/// The server that holds the database, as the conventions name it.
///
/// The process learns this when it opens the pool, never from the statement
/// event. Build it from the connection settings of the driver, never from the
/// connection URL as text: the URL carries the password.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Server {
    /// `db.namespace`: the name of the database on the server.
    pub namespace: Option<String>,
    /// `server.address`: the host of the server.
    pub address: Option<String>,
    /// `server.port`: the port of the server. The conventions ask for it only
    /// when it is not the default port of the system, so [`Self::with_address`]
    /// takes the default one and drops it.
    pub port: Option<u16>,
}

impl Server {
    #[must_use]
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// The host, and the port when it is not `default_port`.
    #[must_use]
    pub fn with_address(
        mut self,
        address: impl Into<String>,
        port: u16,
        default_port: u16,
    ) -> Self {
        self.address = Some(address.into());
        self.port = (port != default_port).then_some(port);
        self
    }

    fn attributes(&self) -> Vec<KeyValue> {
        let mut attributes = Vec::new();
        if let Some(namespace) = &self.namespace {
            attributes.push(KeyValue::new(DB_NAMESPACE, namespace.clone()));
        }
        if let Some(address) = &self.address {
            attributes.push(KeyValue::new(SERVER_ADDRESS, address.clone()));
        }
        if let Some(port) = self.port {
            attributes.push(KeyValue::new(SERVER_PORT, i64::from(port)));
        }
        attributes
    }
}

/// What the statement event cannot tell about the database. The conventions ask
/// for these on every client span, and only the process knows them.
///
/// The system is known when the layer starts. The server is not: the subscriber
/// starts before the process reads its command line, so [`describe`] fills it
/// later. Every clone of one `Database` shares that one answer.
///
/// [`describe`]: Self::describe
#[derive(Clone, Debug)]
pub struct Database {
    /// The value of `db.system.name`. The conventions require it. Use [`systems`].
    system: &'static str,
    server: Arc<OnceLock<Server>>,
}

impl Database {
    /// A database with the system alone.
    pub fn new(system: &'static str) -> Self {
        Self {
            system,
            server: Arc::new(OnceLock::new()),
        }
    }

    pub fn postgresql() -> Self {
        Self::new(systems::POSTGRESQL)
    }

    /// The port that the conventions call the default one for this system.
    pub fn default_port(&self) -> Option<u16> {
        match self.system {
            systems::POSTGRESQL => Some(DEFAULT_POSTGRESQL_PORT),
            systems::MYSQL | systems::MARIADB => Some(DEFAULT_MYSQL_PORT),
            _ => None,
        }
    }

    /// Names the server of this database. The process calls it once, when it
    /// opens the pool. It answers `false` when the server already has a name,
    /// and then changes nothing.
    pub fn describe(&self, server: Server) -> bool {
        self.server.set(server).is_ok()
    }

    /// The server for a database that the caller knows up front, for example in
    /// a test. [`describe`] is the way of a process that learns it later.
    ///
    /// [`describe`]: Self::describe
    #[must_use]
    pub fn with_server(self, server: Server) -> Self {
        self.describe(server);
        self
    }

    /// The server of this database, once something named it.
    pub fn server(&self) -> Option<&Server> {
        self.server.get()
    }

    /// The attributes that every span of this database carries.
    fn attributes(&self) -> Vec<KeyValue> {
        let mut attributes = vec![KeyValue::new(DB_SYSTEM_NAME, Value::from(self.system))];
        if let Some(server) = self.server.get() {
            attributes.extend(server.attributes());
        }
        attributes
    }
}

// endregion: configuration

// region: layer

/// A layer that makes one span per statement of sqlx. Add it to the registry
/// next to the OpenTelemetry layer, with the same tracer.
pub fn layer<S, T>(tracer: T, database: Database) -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    T: Tracer + Send + Sync + 'static,
    T::Span: Send + Sync + 'static,
{
    SqlxSpanLayer {
        tracer,
        database,
        dispatch: OnceLock::new(),
    }
    .with_filter(StatementFilter)
}

/// The filter of the layer.
///
/// The statement event of sqlx is a `DEBUG` event, and the `RUST_LOG` filter of
/// the other layers drops it. The hint raises the maximum level of the registry
/// to `DEBUG`, so that sqlx makes the event at all. The filters of the other
/// layers still drop every other `DEBUG` event.
struct StatementFilter;

impl<S> Filter<S> for StatementFilter {
    fn enabled(&self, metadata: &Metadata<'_>, _context: &Context<'_, S>) -> bool {
        // Every span passes. A per-layer filter also hides the spans that it
        // drops, and the layer needs the span of the caller as the parent of
        // the statement. The layer makes nothing from a span, so a span that
        // passes costs one bit.
        metadata.is_span() || metadata.target() == TARGET
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::DEBUG)
    }
}

struct SqlxSpanLayer<T> {
    tracer: T,
    database: Database,
    /// The subscriber of this layer, from [`Layer::on_register_dispatch`].
    ///
    /// `Span::current()` gives nothing inside `on_event`: `tracing` guards
    /// against a second call into the subscriber, and hands out the no-op
    /// subscriber instead. The statement span would lose its parent and become
    /// its own trace. The dispatch of the registration has no such guard.
    ///
    /// It is a weak handle, so that the layer never keeps the subscriber alive.
    dispatch: OnceLock<WeakDispatch>,
}

impl<T> SqlxSpanLayer<T> {
    /// The OpenTelemetry context of the span that the statement ran in. It is
    /// empty for a statement that ran outside a span, and the statement then
    /// becomes its own trace.
    fn parent_context<S>(
        &self,
        event: &Event<'_>,
        context: &Context<'_, S>,
    ) -> opentelemetry::Context
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        let Some(caller) = context.event_span(event) else {
            return opentelemetry::Context::new();
        };
        let Some(dispatch) = self.dispatch.get().and_then(WeakDispatch::upgrade) else {
            return opentelemetry::Context::new();
        };
        get_otel_context(&caller.id(), &dispatch).unwrap_or_default()
    }
}

impl<S, T> Layer<S> for SqlxSpanLayer<T>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    T: Tracer + Send + Sync + 'static,
    T::Span: Send + Sync + 'static,
{
    fn on_register_dispatch(&self, subscriber: &Dispatch) {
        let _ = self.dispatch.set(subscriber.downgrade());
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let mut statement = Statement::default();
        event.record(&mut statement);

        // The event arrives at the end of the statement, so the start is that
        // much earlier. A clock that cannot go back gives a span of no length,
        // which still sits at the right point of the trace.
        let end = SystemTime::now();
        let start = end.checked_sub(statement.elapsed).unwrap_or(end);

        let text = statement.text();
        let query = Query::of(&text);
        let mut attributes = self.database.attributes();
        attributes.extend(query.attributes());
        attributes.extend(statement.attributes());

        let parent = self.parent_context(event, &context);
        let builder = SpanBuilder::from_name(query.summary())
            .with_kind(SpanKind::Client)
            .with_start_time(start)
            .with_attributes(attributes);
        self.tracer
            .build_with_context(builder, &parent)
            .end_with_timestamp(end);
    }
}

// endregion: layer

// region: event

/// The fields that one statement event carries.
#[derive(Default)]
struct Statement {
    summary: String,
    statement: String,
    rows_returned: u64,
    rows_affected: u64,
    elapsed: Duration,
}

impl Statement {
    /// The whole statement. sqlx sends it in `db.statement` when it cut the
    /// summary, and in the summary alone when the statement is short.
    fn text(&self) -> String {
        let statement = self.statement.trim();
        if statement.is_empty() {
            self.summary
                .trim_end_matches(fields::ELLIPSIS)
                .trim()
                .to_owned()
        } else {
            statement.to_owned()
        }
    }

    fn attributes(&self) -> Vec<KeyValue> {
        vec![
            KeyValue::new(DB_RESPONSE_RETURNED_ROWS, count(self.rows_returned)),
            KeyValue::new(SQLX_ROWS_AFFECTED, count(self.rows_affected)),
        ]
    }
}

/// A row count of the event as the signed number that an attribute holds.
fn count(rows: u64) -> i64 {
    i64::try_from(rows).unwrap_or(i64::MAX)
}

impl Visit for Statement {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            fields::SUMMARY => self.summary = value.to_owned(),
            fields::STATEMENT => self.statement = value.to_owned(),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            fields::ROWS_RETURNED => self.rows_returned = value,
            fields::ROWS_AFFECTED => self.rows_affected = value,
            _ => {}
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == fields::ELAPSED_SECS && value.is_finite() && value > 0.0 {
            self.elapsed = Duration::from_secs_f64(value);
        }
    }

    /// The other fields of the event hold nothing that the span needs. The
    /// elapsed time arrives twice, and `elapsed_secs` is the number of the two.
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

// endregion: event

// region: statement

/// The words of the conventions that one statement gives: the operation, the
/// table, and the summary that names the span.
///
/// The conventions ask for a summary of low cardinality, so the summary holds
/// the operation and the table alone, never the columns or the values. The
/// summary of sqlx is the first four words of the statement, which the columns
/// would fill, so this type reads the statement instead.
struct Query<'a> {
    operation: Option<&'a str>,
    collection: Option<String>,
    text: &'a str,
}

/// The keyword that stands before the table, for each operation that names one.
/// A statement of another operation gets its operation alone.
const COLLECTION_KEYWORDS: [(&str, &str); 4] = [
    ("SELECT", "FROM"),
    ("DELETE", "FROM"),
    ("INSERT", "INTO"),
    ("UPDATE", "UPDATE"),
];

/// The word that opens a statement of common table expressions.
const WITH: &str = "WITH";

/// The character that opens a text literal.
const QUOTE: char = '\'';

/// Two of these open a line comment.
const LINE_COMMENT: char = '-';

/// The operations that can follow the table expressions of a `WITH`. The
/// statement does the work of one of these, and `WITH` names none of them.
const OUTER_OPERATIONS: [&str; 5] = ["SELECT", "INSERT", "UPDATE", "DELETE", "MERGE"];

impl<'a> Query<'a> {
    fn of(text: &'a str) -> Self {
        let words = words_of(text);
        let start = operation_at(&words);
        let operation = words.get(start).map(|word| word.text);
        let collection = operation.and_then(|operation| collection_of(&words[start..], operation));
        Self {
            operation,
            collection,
            text,
        }
    }

    /// `db.query.summary`, and the name of the span: the operation and the
    /// table, as the conventions write it.
    fn summary(&self) -> String {
        match (self.operation, self.collection.as_deref()) {
            (Some(operation), Some(collection)) => format!("{operation} {collection}"),
            (Some(operation), None) => operation.to_owned(),
            (None, _) => UNNAMED_STATEMENT.to_owned(),
        }
    }

    fn attributes(&self) -> Vec<KeyValue> {
        let mut attributes = vec![
            KeyValue::new(DB_QUERY_TEXT, self.text.to_owned()),
            KeyValue::new(DB_QUERY_SUMMARY, self.summary()),
        ];
        if let Some(operation) = self.operation {
            attributes.push(KeyValue::new(DB_OPERATION_NAME, operation.to_owned()));
        }
        if let Some(collection) = &self.collection {
            attributes.push(KeyValue::new(DB_COLLECTION_NAME, collection.clone()));
        }
        attributes
    }
}

/// One word of a statement, with the number of parentheses that hold it.
struct Word<'a> {
    text: &'a str,
    depth: usize,
}

/// The word that carries the operation.
///
/// It is the first word of a plain statement. A statement that opens with
/// `WITH` does the work of the operation that follows its table expressions,
/// so the first word of such a statement names nothing. The table expressions
/// stand inside parentheses, and the operation stands outside every one of
/// them.
fn operation_at(words: &[Word<'_>]) -> usize {
    let Some(first) = words.first() else {
        return 0;
    };
    if !first.text.eq_ignore_ascii_case(WITH) {
        return 0;
    }
    words
        .iter()
        .position(|word| {
            word.depth == 0
                && OUTER_OPERATIONS
                    .iter()
                    .any(|operation| operation.eq_ignore_ascii_case(word.text))
        })
        .unwrap_or(0)
}

/// The table of a statement: the word after the keyword of the operation.
///
/// Only a word outside every parenthesis counts. A table expression and a
/// subquery each carry their own `FROM`, and neither names the table of the
/// statement.
fn collection_of(words: &[Word<'_>], operation: &str) -> Option<String> {
    let (_, keyword) = COLLECTION_KEYWORDS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(operation))?;
    let mut outer = words.iter().filter(|word| word.depth == 0);
    // `UPDATE` is its own keyword, so the table is the word after the first one.
    while let Some(word) = outer.next() {
        if word.text.eq_ignore_ascii_case(keyword) {
            return outer
                .next()
                .map(|word| table_name(word.text))
                .filter(|name| !name.is_empty());
        }
    }
    None
}

/// The words of a statement, each with the depth of parentheses that holds it.
/// A parenthesis, a comma, and a semicolon end a word and never belong to one.
///
/// A comment and a text literal give no word. A migration opens with a line
/// comment, and the span of such a statement would otherwise carry the name
/// `--`. A literal can hold a parenthesis or the two dashes of a comment, and
/// it names no table, so the whole literal goes.
fn words_of(text: &str) -> Vec<Word<'_>> {
    let characters: Vec<(usize, char)> = text.char_indices().collect();
    let mut words = Vec::new();
    let mut depth = 0usize;
    let mut word: Option<(usize, usize)> = None;
    let mut index = 0usize;

    while index < characters.len() {
        let (position, character) = characters[index];
        let next = characters.get(index + 1).map(|(_, character)| *character);

        // A line comment runs to the end of its line.
        if character == LINE_COMMENT && next == Some(LINE_COMMENT) {
            end_word(text, &mut word, position, &mut words);
            while index < characters.len() && characters[index].1 != '\n' {
                index += 1;
            }
            continue;
        }
        // A block comment runs to its close, and PostgreSQL lets one nest.
        if character == '/' && next == Some('*') {
            end_word(text, &mut word, position, &mut words);
            index += 2;
            let mut open = 1usize;
            while index < characters.len() && open > 0 {
                let this = characters[index].1;
                let after = characters.get(index + 1).map(|(_, character)| *character);
                if this == '/' && after == Some('*') {
                    open += 1;
                    index += 2;
                } else if this == '*' && after == Some('/') {
                    open -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            continue;
        }
        // A text literal ends at the next quote. Two quotes in a row are one
        // quote of the text, not the end.
        if character == QUOTE {
            end_word(text, &mut word, position, &mut words);
            index += 1;
            while index < characters.len() {
                if characters[index].1 == QUOTE {
                    if characters.get(index + 1).map(|(_, c)| *c) == Some(QUOTE) {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }

        let ends_the_word = character.is_whitespace()
            || character == '('
            || character == ')'
            || character == ','
            || character == ';';
        if ends_the_word {
            end_word(text, &mut word, position, &mut words);
            match character {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
        } else if word.is_none() {
            word = Some((position, depth));
        }
        index += 1;
    }
    end_word(text, &mut word, text.len(), &mut words);
    words
}

/// Closes the word that is open, if there is one.
fn end_word<'a>(
    text: &'a str,
    word: &mut Option<(usize, usize)>,
    at: usize,
    words: &mut Vec<Word<'a>>,
) {
    if let Some((from, depth)) = word.take() {
        words.push(Word {
            text: &text[from..at],
            depth,
        });
    }
}

/// The name alone: no quote, no parenthesis, no comma, no semicolon. A name
/// with a schema keeps the schema and the dot, which the conventions ask for.
fn table_name(word: &str) -> String {
    word.chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || *character == '.'
                || *character == '_'
                || *character == '$'
        })
        .collect()
}

// endregion: statement

// region: tests

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use opentelemetry::trace::{SpanKind, TracerProvider as _};
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    /// The tracer name of a test.
    const TESTING: &str = "testing";

    #[test]
    fn the_summary_names_the_operation_and_the_table() {
        let cases = [
            ("SELECT id, title FROM items WHERE id = $1", "SELECT items"),
            ("select 1", "select"),
            ("INSERT INTO plays (id) VALUES ($1)", "INSERT plays"),
            ("UPDATE progress SET position_ms = $1", "UPDATE progress"),
            ("DELETE FROM plays WHERE id = $1", "DELETE plays"),
            (
                r#"SELECT * FROM "public"."play_details" p"#,
                "SELECT public.play_details",
            ),
            ("SELECT count(*) FROM items;", "SELECT items"),
            ("BEGIN", "BEGIN"),
            ("", UNNAMED_STATEMENT),
            // A subquery carries its own `FROM`, and it names no table of the
            // statement.
            (
                "SELECT (SELECT max(id) FROM plays) FROM items",
                "SELECT items",
            ),
            // A statement of table expressions does the work of the operation
            // that follows them, never of `WITH`.
            (
                "WITH days AS (SELECT day FROM play_details) SELECT count(*) FROM days",
                "SELECT days",
            ),
            (
                "WITH a AS (SELECT 1), b AS (SELECT 2 FROM a) INSERT INTO plays SELECT * FROM b",
                "INSERT plays",
            ),
            // A `WITH` that names no operation of its own keeps its first word.
            ("WITH days AS (SELECT day FROM play_details)", "WITH"),
        ];
        for (statement, expected) in cases {
            assert_eq!(Query::of(statement).summary(), expected, "{statement}");
        }
    }

    /// The streak query of `storage::statistics`. Its first `FROM` belongs to a
    /// table expression, four parentheses deep in the `FILTER` of the outer
    /// `SELECT`.
    #[test]
    fn the_streak_query_names_its_outer_table() {
        let statement = r#"WITH days AS (
             SELECT DISTINCT (watched_at AT TIME ZONE $1)::date AS day FROM play_details
           ), islands AS (
             SELECT day, day - (ROW_NUMBER() OVER (ORDER BY day))::int AS island FROM days
           ), runs AS (
             SELECT COUNT(*) AS length, MAX(day) AS last_day FROM islands GROUP BY island
           )
           SELECT
             COALESCE(MAX(length), 0) AS "longest!",
             COALESCE(MAX(length) FILTER (WHERE last_day >= ($2 AT TIME ZONE $1)::date - $3::int), 0) AS "current!"
           FROM runs"#;
        let query = Query::of(statement);
        assert_eq!(query.summary(), "SELECT runs");
        assert_eq!(query.operation, Some("SELECT"));
        assert_eq!(query.collection.as_deref(), Some("runs"));
    }

    /// The two other statements of table expressions of the repository. The
    /// `months` query of `storage::statistics` holds a `FROM` in its second
    /// table expression, and the movie recommendation of `catalog` opens with
    /// `FROM unnest(...)`, which names no table at all.
    #[test]
    fn the_other_queries_of_the_repository_name_their_outer_table() {
        let months = r#"WITH bounds AS (
             SELECT date_trunc('month', $2 AT TIME ZONE $1) AS last_month
           ), series AS (
             SELECT generate_series(last_month - make_interval(months => $3), last_month, interval '1 month') AS start
             FROM bounds
           )
           SELECT to_char(s.start, 'YYYY-MM') AS "period!",
                  COUNT(d.id) AS "plays!",
                  COALESCE(SUM(d.runtime_ms), 0)::bigint AS "runtime_ms!"
           FROM series s
           LEFT JOIN play_details d ON date_trunc('month', d.watched_at AT TIME ZONE $1) = s.start
           GROUP BY s.start
           ORDER BY s.start"#;
        let query = Query::of(months);
        assert_eq!(query.operation, Some("SELECT"));
        assert_eq!(query.collection.as_deref(), Some("series"));

        let recommendation = r#"WITH affinity AS (
                 SELECT * FROM unnest($1::bigint[], $2::float8[]) AS t (genre_id, weight)
               ), pool AS (
                 SELECT m.id FROM tmdb_movie m
                 WHERE m.vote_count >= $3 AND NOT (m.id = ANY($4))
                 LIMIT $6
               ), scored AS (
                 SELECT p.id, SUM(COALESCE(a.weight, 0.0)) / sqrt(COUNT(*)::float8) AS affinity
                 FROM pool p
                 JOIN tmdb_movie_genre g ON g.movie_id = p.id
                 GROUP BY p.id
               )
               SELECT m.id, m.title_en, COALESCE(m.weighted_rating, 0.0) AS "quality!"
               FROM scored s JOIN tmdb_movie m ON m.id = s.id
               ORDER BY s.affinity DESC, m.id DESC
               LIMIT $7"#;
        let query = Query::of(recommendation);
        assert_eq!(query.operation, Some("SELECT"));
        assert_eq!(query.collection.as_deref(), Some("scored"));
    }

    /// A comment and a text literal give no word. The migrations of
    /// `storage` open with a line comment, and a literal can hold a
    /// parenthesis or the two dashes of a comment.
    #[test]
    fn the_server_carries_no_port_when_it_is_the_default_one() {
        let default = Server::default().with_address("db.example", 5432, 5432);
        assert_eq!(default.address.as_deref(), Some("db.example"));
        assert_eq!(default.port, None);

        let other = Server::default().with_address("db.example", 6432, 5432);
        assert_eq!(other.port, Some(6432));
    }

    #[test]
    fn a_database_without_a_server_carries_the_system_alone() {
        let attributes = Database::postgresql().attributes();
        let names: Vec<String> = attributes
            .iter()
            .map(|pair| pair.key.as_str().to_owned())
            .collect();
        assert_eq!(names, vec![DB_SYSTEM_NAME.to_owned()]);
    }

    /// The process names the server after the subscriber starts, and every
    /// clone of the database sees the answer.
    #[test]
    fn describe_names_the_server_of_every_clone() {
        let database = Database::postgresql();
        let clone = database.clone();
        assert!(
            database.describe(Server::default().with_namespace("watchkeep").with_address(
                "db.example",
                6432,
                5432
            ))
        );

        let attribute = |key: &str| {
            clone
                .attributes()
                .iter()
                .find(|pair| pair.key.as_str() == key)
                .map(|pair| pair.value.to_string())
        };
        assert_eq!(attribute(DB_NAMESPACE).as_deref(), Some("watchkeep"));
        assert_eq!(attribute(SERVER_ADDRESS).as_deref(), Some("db.example"));
        assert_eq!(attribute(SERVER_PORT).as_deref(), Some("6432"));

        // The server keeps the name it has.
        assert!(!clone.describe(Server::default().with_namespace("other")));
        assert_eq!(attribute(DB_NAMESPACE).as_deref(), Some("watchkeep"));
    }

    #[test]
    fn the_default_port_follows_the_system() {
        assert_eq!(Database::postgresql().default_port(), Some(5432));
        assert_eq!(Database::new(systems::MYSQL).default_port(), Some(3306));
        assert_eq!(Database::new(systems::SQLITE).default_port(), None);
    }

    #[test]
    fn a_comment_and_a_literal_name_nothing() {
        let cases = [
            // The head of `0001_initial.sql`.
            (
                "-- The Watchkeep schema. Ids are UUID v7 from the server.\n\nCREATE TABLE items (id uuid)",
                "CREATE",
            ),
            ("/* a block */ SELECT id FROM items", "SELECT items"),
            (
                "/* a /* nested */ block */ DELETE FROM plays",
                "DELETE plays",
            ),
            ("SELECT id -- FROM comments\n FROM items", "SELECT items"),
            // The literal holds the dashes of a comment and a parenthesis.
            ("SELECT '-- ( ' FROM items", "SELECT items"),
            // Two quotes in a row are one quote of the text, not its end.
            ("SELECT 'it''s ( here' FROM items", "SELECT items"),
            // A single dash is the operator of a subtraction, not a comment.
            ("SELECT day - 1 FROM days", "SELECT days"),
            // A comment alone gives no operation at all.
            ("-- nothing here", UNNAMED_STATEMENT),
        ];
        for (statement, expected) in cases {
            assert_eq!(Query::of(statement).summary(), expected, "{statement}");
        }
    }

    #[test]
    fn a_short_statement_comes_from_the_summary_field() {
        // sqlx leaves `db.statement` empty and cuts the summary with an ellipsis.
        let statement = Statement {
            summary: "SELECT id, title FROM …".to_owned(),
            ..Statement::default()
        };
        assert_eq!(statement.text(), "SELECT id, title FROM");
    }

    /// Emits one statement event of sqlx under a parent span and returns the
    /// spans that the layer made.
    fn spans_of_one_statement() -> Vec<SpanData> {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let tracer = provider.tracer(TESTING);
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(tracer.clone()))
            .with(layer(
                tracer,
                Database::postgresql().with_server(Server::default().with_namespace("watchkeep")),
            ));

        ::tracing::subscriber::with_default(subscriber, || {
            let parent = ::tracing::info_span!("caller");
            let _entered = parent.enter();
            ::tracing::event!(
                target: TARGET,
                ::tracing::Level::DEBUG,
                summary = "SELECT id, title FROM …".to_owned(),
                db.statement = "\n\nSELECT id, title FROM items WHERE id = $1\n".to_owned(),
                rows_affected = 0_u64,
                rows_returned = 3_u64,
                elapsed_secs = 0.25_f64,
            );
        });

        provider.force_flush().expect("the spans flush");
        exporter.get_finished_spans().expect("the spans are there")
    }

    #[test]
    fn the_layer_makes_a_client_span_from_a_statement_event() {
        let spans = spans_of_one_statement();
        let span = spans
            .iter()
            .find(|span| span.name == "SELECT items")
            .expect("the statement made a span");

        assert_eq!(span.span_kind, SpanKind::Client);

        let attribute = |key: &str| {
            span.attributes
                .iter()
                .find(|pair| pair.key.as_str() == key)
                .map(|pair| pair.value.to_string())
        };
        assert_eq!(attribute(DB_SYSTEM_NAME).as_deref(), Some("postgresql"));
        assert_eq!(attribute(DB_NAMESPACE).as_deref(), Some("watchkeep"));
        assert_eq!(attribute(DB_OPERATION_NAME).as_deref(), Some("SELECT"));
        assert_eq!(attribute(DB_COLLECTION_NAME).as_deref(), Some("items"));
        assert_eq!(attribute(DB_QUERY_SUMMARY).as_deref(), Some("SELECT items"));
        assert_eq!(
            attribute(DB_QUERY_TEXT).as_deref(),
            Some("SELECT id, title FROM items WHERE id = $1")
        );
        assert_eq!(attribute(DB_RESPONSE_RETURNED_ROWS).as_deref(), Some("3"));
    }

    #[test]
    fn the_span_lasts_as_long_as_the_statement() {
        let spans = spans_of_one_statement();
        let span = spans
            .iter()
            .find(|span| span.name == "SELECT items")
            .expect("the statement made a span");
        let elapsed = span
            .end_time
            .duration_since(span.start_time)
            .expect("the span ends after it starts");
        assert!(
            elapsed >= Duration::from_millis(200),
            "the span lasted {elapsed:?}, not the 250ms of the event"
        );
    }

    #[test]
    fn the_caller_is_the_parent_of_the_statement() {
        let spans = spans_of_one_statement();
        let statement = spans
            .iter()
            .find(|span| span.name == "SELECT items")
            .expect("the statement made a span");
        let caller = spans
            .iter()
            .find(|span| span.name == "caller")
            .expect("the caller made a span");
        assert_eq!(statement.parent_span_id, caller.span_context.span_id());
        assert_eq!(
            statement.span_context.trace_id(),
            caller.span_context.trace_id()
        );
    }
}

// endregion: tests
