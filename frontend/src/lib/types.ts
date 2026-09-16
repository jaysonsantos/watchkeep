/**
 * The shapes of the JSON API. ts-rs generates them from the Rust structs into `generated/`.
 * Regenerate with `cargo test --workspace --lib export_bindings`; prek checks that they are current.
 */

export type { AddMediaBody } from "./generated/AddMediaBody.ts";
export type { BucketCount } from "./generated/BucketCount.ts";
export type { CatalogMovie } from "./generated/CatalogMovie.ts";
export type { CatalogShow } from "./generated/CatalogShow.ts";
export type { HistoryEntry } from "./generated/HistoryEntry.ts";
export type { HistoryPage } from "./generated/HistoryPage.ts";
export type { LabelCount } from "./generated/LabelCount.ts";
export type { MediaKind } from "./generated/MediaKind.ts";
export type { MediaRow } from "./generated/MediaRow.ts";
export type { MergedEpisode } from "./generated/MergedEpisode.ts";
export type { MovieDetail } from "./generated/MovieDetail.ts";
export type { MovieView } from "./generated/MovieView.ts";
export type { PeriodCount } from "./generated/PeriodCount.ts";
export type { PlayState } from "./generated/PlayState.ts";
export type { PlayTotals } from "./generated/PlayTotals.ts";
export type { ProgressView } from "./generated/ProgressView.ts";
export type { Recommendation } from "./generated/Recommendation.ts";
export type { Recommendations } from "./generated/Recommendations.ts";
export type { SearchResult } from "./generated/SearchResult.ts";
export type { SearchResults } from "./generated/SearchResults.ts";
export type { ServerConfig } from "./generated/ServerConfig.ts";
export type { ShowListItem } from "./generated/ShowListItem.ts";
export type { ShowPage } from "./generated/ShowPage.ts";
export type { SortOrder } from "./generated/SortOrder.ts";
export type { Statistics } from "./generated/Statistics.ts";
export type { Stats } from "./generated/Stats.ts";
export type { Streak } from "./generated/Streak.ts";
export type { TargetKind } from "./generated/TargetKind.ts";
export type { TasteProfile } from "./generated/TasteProfile.ts";
export type { TasteShare } from "./generated/TasteShare.ts";
export type { TopItem } from "./generated/TopItem.ts";
export type { WatchFilter } from "./generated/WatchFilter.ts";
export type { WatchlistItem } from "./generated/WatchlistItem.ts";
export type { WebhookLogEntry } from "./generated/WebhookLogEntry.ts";

/** Row ids are UUID v7 strings. They sort by creation time. */
export type Id = string;
