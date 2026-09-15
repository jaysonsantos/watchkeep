/** Calls to the JSON API of the Rust server. */

import {
  filterOf,
  LIST_PAGE_SIZE,
  type ListState,
  pageOf,
  QUERY,
  type SortOrder,
  sortOf,
  type WatchFilter,
} from "./lists.ts";
import type { Id } from "./types.ts";

/** The prefix of every API route. */
export const API_BASE = "/api";

/** The number of matches before `limit` and `offset`, on list routes. */
export const TOTAL_COUNT_HEADER = "x-total-count";

export const JSON_TYPE = "application/json";

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

type Fetch = typeof fetch;

async function errorMessage(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { error?: unknown };
    if (typeof body?.error === "string") return body.error;
  } catch {
    // The body is not JSON.
  }
  return response.statusText || `HTTP ${response.status}`;
}

/** GET a JSON document. `total` is the count header of list routes. */
export async function getJson<T>(fetchImpl: Fetch, path: string): Promise<{ data: T; total: number | null }> {
  const response = await fetchImpl(path, { headers: { accept: JSON_TYPE } });
  if (!response.ok) throw new ApiError(response.status, await errorMessage(response));
  const total = response.headers.get(TOTAL_COUNT_HEADER);
  return { data: (await response.json()) as T, total: total === null ? null : Number(total) };
}

/** POST or DELETE with an optional JSON body. */
export async function send<T = unknown>(method: "POST" | "DELETE", path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: body === undefined ? { accept: JSON_TYPE } : { accept: JSON_TYPE, "content-type": JSON_TYPE },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) throw new ApiError(response.status, await errorMessage(response));
  return (await response.json()) as T;
}

export interface ListQuery {
  filter: WatchFilter;
  search: string;
  sort: SortOrder;
}

export function listParams(params: URLSearchParams): ListQuery {
  return {
    filter: filterOf(params.get(QUERY.status)),
    search: params.get(QUERY.search) ?? "",
    sort: sortOf(params.get(QUERY.sort)),
  };
}

/**
 * One page of a movie or show list. A page past the end loads the last page,
 * so the page links never point at an empty page.
 */
export async function loadList<T>(
  fetchImpl: Fetch,
  base: string,
  query: ListQuery,
  pageParam: string | null,
): Promise<{ items: T[]; total: number; state: ListState }> {
  const url = (page: number) => {
    const params = new URLSearchParams();
    params.set(QUERY.status, query.filter);
    if (query.search) params.set(QUERY.search, query.search);
    params.set(QUERY.sort, query.sort);
    params.set(QUERY.limit, String(LIST_PAGE_SIZE));
    params.set(QUERY.offset, String((page - 1) * LIST_PAGE_SIZE));
    return `${base}?${params}`;
  };
  let page = Math.max(1, Math.floor(Number(pageParam ?? 1)) || 1);
  let result = await getJson<T[]>(fetchImpl, url(page));
  const total = result.total ?? result.data.length;
  const clamped = pageOf(String(page), total);
  if (clamped !== page) {
    page = clamped;
    result = await getJson<T[]>(fetchImpl, url(page));
  }
  return { items: result.data, total, state: { ...query, page, pageSize: LIST_PAGE_SIZE } };
}

/** The buttons of the UI. Each one maps to one API call. */
export type ActionName =
  | "watch-movie"
  | "unwatch-movie"
  | "watch-episode"
  | "unwatch-episode"
  | "watch-episode-number"
  | "unwatch-episode-number"
  | "watch-show"
  | "unwatch-show"
  | "remove-play"
  | "watchlist-add-movie"
  | "watchlist-remove-movie"
  | "watchlist-add-show"
  | "watchlist-remove-show"
  | "hide-show"
  | "unhide-show"
  | "clear-progress"
  | "sync";

export interface ActionRequest {
  method: "POST" | "DELETE";
  path: string;
}

/** The API call behind a button. `id` is the row the button acts on; `sync` needs none. */
export function actionRequest(
  action: ActionName,
  id: Id | null,
  fields: Record<string, string | number> = {},
): ActionRequest {
  switch (action) {
    case "watch-movie":
      return { method: "POST", path: `${API_BASE}/movies/${id}/watched` };
    case "unwatch-movie":
      return { method: "DELETE", path: `${API_BASE}/movies/${id}/watched` };
    case "watch-episode":
      return { method: "POST", path: `${API_BASE}/episodes/${id}/watched` };
    case "unwatch-episode":
      return { method: "DELETE", path: `${API_BASE}/episodes/${id}/watched` };
    case "watch-episode-number":
      return {
        method: "POST",
        path: `${API_BASE}/shows/${id}/seasons/${fields.season}/episodes/${fields.number}/watched`,
      };
    case "unwatch-episode-number":
      return {
        method: "DELETE",
        path: `${API_BASE}/shows/${id}/seasons/${fields.season}/episodes/${fields.number}/watched`,
      };
    case "watch-show":
      return { method: "POST", path: `${API_BASE}/shows/${id}/watched` };
    case "unwatch-show":
      return { method: "DELETE", path: `${API_BASE}/shows/${id}/watched` };
    case "remove-play":
      return { method: "DELETE", path: `${API_BASE}/history/${id}` };
    case "watchlist-add-movie":
      return { method: "POST", path: `${API_BASE}/movies/${id}/watchlist` };
    case "watchlist-remove-movie":
      return { method: "DELETE", path: `${API_BASE}/movies/${id}/watchlist` };
    case "watchlist-add-show":
      return { method: "POST", path: `${API_BASE}/shows/${id}/watchlist` };
    case "watchlist-remove-show":
      return { method: "DELETE", path: `${API_BASE}/shows/${id}/watchlist` };
    case "hide-show":
      return { method: "POST", path: `${API_BASE}/shows/${id}/hidden` };
    case "unhide-show":
      return { method: "DELETE", path: `${API_BASE}/shows/${id}/hidden` };
    case "clear-progress":
      return { method: "DELETE", path: `${API_BASE}/progress/${fields.kind}/${id}` };
    case "sync":
      return { method: "POST", path: `${API_BASE}/sync` };
  }
}

export async function act(
  action: ActionName,
  id: Id | null,
  fields: Record<string, string | number> = {},
): Promise<void> {
  const request = actionRequest(action, id, fields);
  await send(request.method, request.path);
}
