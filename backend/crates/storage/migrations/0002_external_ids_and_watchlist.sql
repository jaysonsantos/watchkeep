-- Plays from an import carry the id of the source record, so a second import adds no duplicate.
ALTER TABLE plays ADD COLUMN external_id TEXT;
CREATE UNIQUE INDEX plays_source_external ON plays (source, external_id) WHERE external_id IS NOT NULL;

-- Hidden shows do not appear as unwatched.
ALTER TABLE media ADD COLUMN hidden_at TEXT;

CREATE TABLE watchlist (
  target_kind TEXT NOT NULL CHECK (target_kind IN ('movie', 'show')),
  target_id BIGINT NOT NULL,
  listed_at TEXT NOT NULL,
  rank INTEGER,
  PRIMARY KEY (target_kind, target_id)
);
