/** State and small computations of the statistics page. */

import type { BucketCount } from "./types.ts";

const FULL_PERCENT = 100;

/** The IANA time zone of the browser. The API buckets the plays by it. */
export function browserTimezone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone ?? "";
}

/** The busiest bucket, or `null` while no bucket has a play. A tie goes to the first. */
export function peakBucket(buckets: BucketCount[]): BucketCount | null {
  let peak: BucketCount | null = null;
  for (const bucket of buckets) {
    if (bucket.plays > 0 && (peak === null || bucket.plays > peak.plays)) peak = bucket;
  }
  return peak;
}

/** The share of a part in the whole, rounded to a whole percent. */
export function sharePercent(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * FULL_PERCENT) : 0;
}
