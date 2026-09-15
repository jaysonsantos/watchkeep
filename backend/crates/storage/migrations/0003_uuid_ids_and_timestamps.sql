-- Ids become UUID v7 and timestamps become timestamptz. Rows from before this
-- migration get ids that encode their creation time, so the order of ids
-- follows the order of the rows. The Node version stored ISO-8601 text, and
-- ::timestamptz reads it.
--
-- uuidv7() is built into PostgreSQL 18. uuidv7(shift) moves the time inside the
-- id by an interval, so `created_at - clock_timestamp()` puts the creation
-- time of a row into its new id.


-- New id columns, filled from the old ones.
ALTER TABLE media ADD COLUMN uid uuid;
UPDATE media SET uid = uuidv7(created_at::timestamptz - clock_timestamp());
ALTER TABLE media ALTER COLUMN uid SET NOT NULL;

ALTER TABLE episodes ADD COLUMN uid uuid, ADD COLUMN show_uid uuid;
UPDATE episodes e SET uid = uuidv7(e.created_at::timestamptz - clock_timestamp()), show_uid = m.uid FROM media m WHERE m.id = e.show_id;
DELETE FROM episodes WHERE show_uid IS NULL;
ALTER TABLE episodes ALTER COLUMN uid SET NOT NULL, ALTER COLUMN show_uid SET NOT NULL;

ALTER TABLE plays ADD COLUMN uid uuid, ADD COLUMN target_uid uuid;
UPDATE plays SET uid = uuidv7(watched_at::timestamptz - clock_timestamp());
UPDATE plays p SET target_uid = m.uid FROM media m WHERE p.target_kind = 'movie' AND p.target_id = m.id;
UPDATE plays p SET target_uid = e.uid FROM episodes e WHERE p.target_kind = 'episode' AND p.target_id = e.id;
DELETE FROM plays WHERE target_uid IS NULL;
ALTER TABLE plays ALTER COLUMN uid SET NOT NULL, ALTER COLUMN target_uid SET NOT NULL;

ALTER TABLE progress ADD COLUMN target_uid uuid;
UPDATE progress p SET target_uid = m.uid FROM media m WHERE p.target_kind = 'movie' AND p.target_id = m.id;
UPDATE progress p SET target_uid = e.uid FROM episodes e WHERE p.target_kind = 'episode' AND p.target_id = e.id;
DELETE FROM progress WHERE target_uid IS NULL;
ALTER TABLE progress ALTER COLUMN target_uid SET NOT NULL;

ALTER TABLE ratings ADD COLUMN target_uid uuid;
UPDATE ratings r SET target_uid = m.uid FROM media m WHERE r.target_kind IN ('movie', 'show') AND r.target_id = m.id;
UPDATE ratings r SET target_uid = e.uid FROM episodes e WHERE r.target_kind = 'episode' AND r.target_id = e.id;
DELETE FROM ratings WHERE target_uid IS NULL;
ALTER TABLE ratings ALTER COLUMN target_uid SET NOT NULL;

ALTER TABLE watchlist ADD COLUMN target_uid uuid;
UPDATE watchlist w SET target_uid = m.uid FROM media m WHERE w.target_id = m.id;
DELETE FROM watchlist WHERE target_uid IS NULL;
ALTER TABLE watchlist ALTER COLUMN target_uid SET NOT NULL;

ALTER TABLE webhook_events ADD COLUMN uid uuid;
UPDATE webhook_events SET uid = uuidv7(received_at::timestamptz - clock_timestamp());
ALTER TABLE webhook_events ALTER COLUMN uid SET NOT NULL;

-- Swap the id columns. Dropping a serial column drops its sequence, its primary key, and its indexes.
ALTER TABLE episodes DROP CONSTRAINT episodes_show_id_fkey;
ALTER TABLE episodes DROP CONSTRAINT episodes_show_id_season_number_key;

ALTER TABLE media DROP COLUMN id;
ALTER TABLE media RENAME COLUMN uid TO id;
ALTER TABLE media ADD PRIMARY KEY (id);

ALTER TABLE episodes DROP COLUMN id, DROP COLUMN show_id;
ALTER TABLE episodes RENAME COLUMN uid TO id;
ALTER TABLE episodes RENAME COLUMN show_uid TO show_id;
ALTER TABLE episodes ADD PRIMARY KEY (id);
ALTER TABLE episodes ADD FOREIGN KEY (show_id) REFERENCES media (id) ON DELETE CASCADE;
ALTER TABLE episodes ADD UNIQUE (show_id, season, number);

ALTER TABLE plays DROP COLUMN id, DROP COLUMN target_id;
ALTER TABLE plays RENAME COLUMN uid TO id;
ALTER TABLE plays RENAME COLUMN target_uid TO target_id;
ALTER TABLE plays ADD PRIMARY KEY (id);
CREATE INDEX plays_target ON plays (target_kind, target_id, watched_at);

ALTER TABLE progress DROP COLUMN target_id;
ALTER TABLE progress RENAME COLUMN target_uid TO target_id;
ALTER TABLE progress ADD PRIMARY KEY (target_kind, target_id);

ALTER TABLE ratings DROP COLUMN target_id;
ALTER TABLE ratings RENAME COLUMN target_uid TO target_id;
ALTER TABLE ratings ADD PRIMARY KEY (target_kind, target_id);

ALTER TABLE watchlist DROP COLUMN target_id;
ALTER TABLE watchlist RENAME COLUMN target_uid TO target_id;
ALTER TABLE watchlist ADD PRIMARY KEY (target_kind, target_id);

ALTER TABLE webhook_events DROP COLUMN id;
ALTER TABLE webhook_events RENAME COLUMN uid TO id;
ALTER TABLE webhook_events ADD PRIMARY KEY (id);

-- Timestamps and dates.
ALTER TABLE media
  ALTER COLUMN created_at TYPE timestamptz USING created_at::timestamptz,
  ALTER COLUMN updated_at TYPE timestamptz USING updated_at::timestamptz,
  ALTER COLUMN hidden_at TYPE timestamptz USING hidden_at::timestamptz;
ALTER TABLE episodes
  ALTER COLUMN created_at TYPE timestamptz USING created_at::timestamptz,
  ALTER COLUMN updated_at TYPE timestamptz USING updated_at::timestamptz,
  ALTER COLUMN aired_at TYPE date USING (CASE WHEN aired_at ~ '^\d{4}-\d{2}-\d{2}' THEN substr(aired_at, 1, 10)::date END);
ALTER TABLE plays ALTER COLUMN watched_at TYPE timestamptz USING watched_at::timestamptz;
ALTER TABLE progress ALTER COLUMN updated_at TYPE timestamptz USING updated_at::timestamptz;
ALTER TABLE ratings ALTER COLUMN rated_at TYPE timestamptz USING rated_at::timestamptz;
ALTER TABLE watchlist ALTER COLUMN listed_at TYPE timestamptz USING listed_at::timestamptz;
ALTER TABLE webhook_events ALTER COLUMN received_at TYPE timestamptz USING received_at::timestamptz;

-- A row without an id gets one from the server.
ALTER TABLE media ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE episodes ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE plays ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE webhook_events ALTER COLUMN id SET DEFAULT uuidv7();
