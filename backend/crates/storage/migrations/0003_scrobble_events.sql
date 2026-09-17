CREATE TABLE scrobble_events (
    event_id uuid PRIMARY KEY,
    target_kind text NOT NULL CHECK (target_kind IN ('movie', 'episode')),
    target_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL
);

CREATE INDEX scrobble_events_target
    ON scrobble_events (target_kind, target_id, occurred_at DESC);
