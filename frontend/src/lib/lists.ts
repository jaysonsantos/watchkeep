/** Filter, sort, and page state of the movie and show lists. The pages and the API client both use this module. */

import type { SortOrder, WatchFilter } from "./types.ts";

export type { SortOrder, WatchFilter };

export const SORT_ORDERS: ReadonlyArray<[SortOrder, string]> = [
  ["recent", "Last watched"],
  ["title", "Title"],
  ["year", "Year"],
  ["added", "Recently added"],
];

export const DEFAULT_FILTER: WatchFilter = "all";
export const DEFAULT_SORT: SortOrder = "recent";

/** Query string keys of the list pages and of the list routes of the API. */
export const QUERY = {
  status: "status",
  search: "q",
  sort: "sort",
  page: "page",
  limit: "limit",
  offset: "offset",
} as const;

export const LIST_PAGE_SIZE = 60;

export interface ListState {
  filter: WatchFilter;
  search: string;
  sort: SortOrder;
  page: number;
  pageSize: number;
}

export function filterOf(value: string | null | undefined): WatchFilter {
  return value === "watched" || value === "unwatched" ? value : DEFAULT_FILTER;
}

export function sortOf(value: string | null | undefined): SortOrder {
  return SORT_ORDERS.some(([sort]) => sort === value) ? (value as SortOrder) : DEFAULT_SORT;
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
  if (filter !== DEFAULT_FILTER) params.set(QUERY.status, filter);
  if (state.search) params.set(QUERY.search, state.search);
  if (state.sort !== DEFAULT_SORT) params.set(QUERY.sort, state.sort);
  if (page > 1) params.set(QUERY.page, String(page));
  const query = params.toString().replaceAll("+", "%20");
  return query ? `${base}?${query}` : base;
}
