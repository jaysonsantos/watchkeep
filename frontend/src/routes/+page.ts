import { getJson } from "$lib/api.ts";
import type { HistoryPage, ProgressView, Stats } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch }) => {
  const [stats, inProgress, recent] = await Promise.all([
    getJson<Stats>(fetch, "/api/stats"),
    getJson<ProgressView[]>(fetch, "/api/progress"),
    getJson<HistoryPage>(fetch, "/api/history?limit=15"),
  ]);
  return { stats: stats.data, inProgress: inProgress.data, recent: recent.data.items };
};
