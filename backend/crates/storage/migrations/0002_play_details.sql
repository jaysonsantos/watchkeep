-- One row per play, with the title, the show, and the runtime that the play
-- consumed. The statistics queries read it, so that each one carries the
-- aggregate and not the joins. An episode without a runtime of its own falls
-- back to the runtime of its show, which Plex fills with the episode average.

CREATE VIEW play_details AS
SELECT p.id,
       p.target_kind,
       p.target_id,
       p.watched_at,
       p.source,
       p.account,
       p.player,
       COALESCE(m.id, s.id) AS media_id,
       COALESCE(m.title, s.title) AS media_title,
       COALESCE(m.year, s.year) AS media_year,
       COALESCE(m.poster_path, s.poster_path) AS poster_path,
       CASE p.target_kind
         WHEN 'movie' THEN m.duration_ms
         ELSE COALESCE(e.duration_ms, s.duration_ms)
       END AS runtime_ms
FROM plays p
LEFT JOIN media m ON p.target_kind = 'movie' AND m.id = p.target_id
LEFT JOIN episodes e ON p.target_kind = 'episode' AND e.id = p.target_id
LEFT JOIN media s ON s.id = e.show_id;
