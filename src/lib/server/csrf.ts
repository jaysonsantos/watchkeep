const FORM_TYPES = new Set(["application/x-www-form-urlencoded", "multipart/form-data", "text/plain"]);
const UNSAFE_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

/**
 * An origin check for form posts to the UI pages. The SvelteKit check is off,
 * because Plex posts webhook forms without an Origin header.
 *
 * Only the host is compared. Without WATCHKEEP_HTTP_ORIGIN, adapter-node
 * assumes https, so a plain-http install would fail a full origin comparison.
 */
export function isCrossSiteFormPost(request: Request, host: string): boolean {
  if (!UNSAFE_METHODS.has(request.method)) return false;
  const type = (request.headers.get("content-type") ?? "").split(";")[0]!.trim().toLowerCase();
  if (!FORM_TYPES.has(type)) return false;
  const origin = request.headers.get("origin");
  if (!origin) return true;
  try {
    return new URL(origin).host !== host;
  } catch {
    return true;
  }
}
