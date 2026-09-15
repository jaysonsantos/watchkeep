import { getJson } from "$lib/api.ts";
import { pageOf } from "$lib/lists.ts";
import type { HistoryPage } from "$lib/types.ts";
import type { PageLoad } from "./$types";

const PAGE_SIZE = 50;

export const load: PageLoad = async ({ fetch, url }) => {
  const history = (page: number) =>
    getJson<HistoryPage>(fetch, `/api/history?limit=${PAGE_SIZE}&offset=${(page - 1) * PAGE_SIZE}`);
  let page = Math.max(1, Math.floor(Number(url.searchParams.get("page") ?? 1)) || 1);
  let result = await history(page);
  const clamped = pageOf(String(page), result.data.total, PAGE_SIZE);
  if (clamped !== page) {
    page = clamped;
    result = await history(page);
  }
  return { entries: result.data.items, page, pageSize: PAGE_SIZE, total: result.data.total };
};
