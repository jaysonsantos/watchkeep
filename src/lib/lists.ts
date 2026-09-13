/** Filter, sort, and page state of the movie and show lists. The server and the browser both use this module. */

export type WatchFilter = "all" | "watched" | "unwatched";

/** List order for movies and shows. `recent` puts the last watched first, then the newest additions. */
export type SortOrder = "recent" | "title" | "year" | "added";

export const SORT_ORDERS: ReadonlyArray<[SortOrder, string]> = [
  ["recent", "Last watched"],
  ["title", "Title"],
  ["year", "Year"],
  ["added", "Recently added"],
];

export const LIST_PAGE_SIZE = 60;

export interface ListState {
  filter: WatchFilter;
  search: string;
  sort: SortOrder;
  page: number;
  pageSize: number;
}

export function filterOf(value: string | null | undefined): WatchFilter {
  return value === "watched" || value === "unwatched" ? value : "all";
}

export function sortOf(value: string | null | undefined): SortOrder {
  return SORT_ORDERS.some(([sort]) => sort === value) ? (value as SortOrder) : "recent";
}

/** The requested page, clamped to the pages that exist. */
export function pageOf(value: string | null | undefined, total: number, pageSize = LIST_PAGE_SIZE): number {
  const last = Math.max(1, Math.ceil(total / pageSize));
  return Math.min(last, Math.max(1, Math.floor(Number(value ?? 1)) || 1));
}

export function pageCount(total: number, pageSize: number): number {
  return Math.max(1, Math.ceil(total / pageSize));
}

/** A list URL that leaves out default values. */
export function listUrl(base: string, state: ListState, page = state.page, filter = state.filter): string {
  const params = new URLSearchParams();
  if (filter !== "all") params.set("status", filter);
  if (state.search) params.set("q", state.search);
  if (state.sort !== "recent") params.set("sort", state.sort);
  if (page > 1) params.set("page", String(page));
  const query = params.toString().replaceAll("+", "%20");
  return query ? `${base}?${query}` : base;
}
