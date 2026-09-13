/**
 * Parse a Plex webhook payload into a normalized event.
 * Reference: https://support.plex.tv/articles/115002267687-webhooks/
 */

export type PlexEventName =
  | "media.play"
  | "media.pause"
  | "media.resume"
  | "media.stop"
  | "media.scrobble"
  | "media.rate";

export const HANDLED_EVENTS: ReadonlySet<string> = new Set<PlexEventName>([
  "media.play",
  "media.pause",
  "media.resume",
  "media.stop",
  "media.scrobble",
  "media.rate",
]);

export interface ExternalIds {
  plexGuid: string | null;
  imdb: string | null;
  tmdb: string | null;
  tvdb: string | null;
}

export interface MovieRef {
  type: "movie";
  title: string;
  year: number | null;
  ids: ExternalIds;
  durationMs: number | null;
  summary: string | null;
  posterPath?: string | null;
}

export interface EpisodeRef {
  type: "episode";
  title: string | null;
  season: number;
  number: number;
  ids: ExternalIds;
  durationMs: number | null;
  airedAt: string | null;
  show: {
    title: string;
    year: number | null;
    ids: ExternalIds;
    posterPath?: string | null;
    summary?: string | null;
  };
}

export type MediaRef = MovieRef | EpisodeRef;

export interface PlexEvent {
  event: PlexEventName;
  account: string | null;
  accountId: string | null;
  player: string | null;
  media: MediaRef;
  /** Playback position in milliseconds when Plex reports one. */
  viewOffsetMs: number | null;
  /** Rating between 0 and 10 for media.rate events. */
  rating: number | null;
}

export type ParseResult =
  | { ok: true; event: PlexEvent }
  | { ok: false; reason: string; event: string | null; mediaType: string | null; title: string | null };

interface PlexGuid {
  id?: string;
}

interface PlexMetadata {
  type?: string;
  title?: string;
  year?: number;
  guid?: string;
  Guid?: PlexGuid[];
  duration?: number;
  viewOffset?: number;
  summary?: string;
  index?: number;
  parentIndex?: number;
  parentGuid?: string;
  grandparentGuid?: string;
  grandparentTitle?: string;
  grandparentYear?: number;
  originallyAvailableAt?: string;
  userRating?: number;
  ratingKey?: string;
  grandparentRatingKey?: string;
}

interface PlexPayload {
  event?: string;
  rating?: number;
  Account?: { id?: number | string; title?: string };
  Player?: { title?: string; uuid?: string };
  Metadata?: PlexMetadata;
}

const LEGACY_AGENTS: Record<string, keyof Omit<ExternalIds, "plexGuid">> = {
  "com.plexapp.agents.imdb": "imdb",
  "com.plexapp.agents.themoviedb": "tmdb",
  "com.plexapp.agents.thetvdb": "tvdb",
  "tv.plex.agents.movie": "tmdb",
  "tv.plex.agents.series": "tmdb",
};

/**
 * Extract external ids from a Plex `guid` string and `Guid` list.
 * Modern agents use `plex://movie/<id>` plus `Guid: [{id: "imdb://tt..."}]`.
 * Legacy agents encode the id in the guid, for example
 * `com.plexapp.agents.thetvdb://12345/1/5?lang=en`.
 */
export function parseIds(guid: string | undefined, guids: PlexGuid[] | undefined): ExternalIds {
  const ids: ExternalIds = { plexGuid: null, imdb: null, tmdb: null, tvdb: null };
  if (guid?.startsWith("plex://")) {
    ids.plexGuid = guid;
  } else if (guid) {
    const match = /^([a-z.]+):\/\/([^/?]+)/i.exec(guid);
    const provider = match?.[1] ? LEGACY_AGENTS[match[1]] : undefined;
    if (provider && match?.[2]) {
      ids.plexGuid = guid.split("?")[0] ?? guid;
      ids[provider] = match[2];
    } else {
      ids.plexGuid = guid;
    }
  }
  for (const entry of guids ?? []) {
    const match = /^(imdb|tmdb|tvdb):\/\/(.+)$/.exec(entry.id ?? "");
    if (!match) continue;
    const provider = match[1] as "imdb" | "tmdb" | "tvdb";
    if (!ids[provider]) ids[provider] = match[2] ?? null;
  }
  return ids;
}

/** Ids of the show that owns a legacy episode guid, for example `...thetvdb://12345/1/5`. */
function legacyShowIds(episodeGuid: string | undefined): ExternalIds {
  const ids: ExternalIds = { plexGuid: null, imdb: null, tmdb: null, tvdb: null };
  if (!episodeGuid || episodeGuid.startsWith("plex://")) return ids;
  const match = /^([a-z.]+):\/\/([^/?]+)/i.exec(episodeGuid);
  const provider = match?.[1] ? LEGACY_AGENTS[match[1]] : undefined;
  if (provider && match?.[2]) {
    ids[provider] = match[2];
    ids.plexGuid = `${match[1]}://${match[2]}`;
  }
  return ids;
}

function optionalNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

export function parseMedia(metadata: PlexMetadata): MediaRef | null {
  if (metadata.type === "movie") {
    const title = optionalString(metadata.title);
    if (!title) return null;
    return {
      type: "movie",
      title,
      year: optionalNumber(metadata.year),
      ids: parseIds(metadata.guid, metadata.Guid),
      durationMs: optionalNumber(metadata.duration),
      summary: optionalString(metadata.summary),
    };
  }
  if (metadata.type === "episode") {
    const showTitle = optionalString(metadata.grandparentTitle);
    const season = optionalNumber(metadata.parentIndex);
    const number = optionalNumber(metadata.index);
    if (!showTitle || season === null || number === null) return null;
    const showIds = metadata.grandparentGuid
      ? parseIds(metadata.grandparentGuid, undefined)
      : legacyShowIds(metadata.guid);
    return {
      type: "episode",
      title: optionalString(metadata.title),
      season,
      number,
      ids: parseIds(metadata.guid, metadata.Guid),
      durationMs: optionalNumber(metadata.duration),
      airedAt: optionalString(metadata.originallyAvailableAt),
      show: {
        title: showTitle,
        year: optionalNumber(metadata.grandparentYear),
        ids: showIds,
      },
    };
  }
  return null;
}

export function parsePlexPayload(raw: unknown): ParseResult {
  if (typeof raw !== "object" || raw === null) {
    return { ok: false, reason: "payload is not an object", event: null, mediaType: null, title: null };
  }
  const payload = raw as PlexPayload;
  const event = optionalString(payload.event);
  const mediaType = optionalString(payload.Metadata?.type);
  const title = optionalString(payload.Metadata?.title);
  if (!event) return { ok: false, reason: "missing event", event: null, mediaType, title };
  if (!HANDLED_EVENTS.has(event)) {
    return { ok: false, reason: `event ${event} is not tracked`, event, mediaType, title };
  }
  if (!payload.Metadata) return { ok: false, reason: "missing Metadata", event, mediaType, title };
  const media = parseMedia(payload.Metadata);
  if (!media) {
    return { ok: false, reason: `media type ${mediaType ?? "unknown"} is not tracked`, event, mediaType, title };
  }
  const rating = optionalNumber(payload.rating) ?? optionalNumber(payload.Metadata.userRating);
  return {
    ok: true,
    event: {
      event: event as PlexEventName,
      account: optionalString(payload.Account?.title),
      accountId: payload.Account?.id === undefined ? null : String(payload.Account.id),
      player: optionalString(payload.Player?.title),
      media,
      viewOffsetMs: optionalNumber(payload.Metadata.viewOffset),
      rating,
    },
  };
}
