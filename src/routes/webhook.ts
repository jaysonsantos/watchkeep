import { Hono } from "hono";
import type { AppContext } from "../app.ts";
import { parsePlexPayload } from "../plex/payload.ts";

const MAX_LOGGED_PAYLOAD = 64 * 1024;

/**
 * Plex sends `multipart/form-data` with a `payload` field that holds JSON.
 * Some proxies forward plain JSON. Both shapes are accepted.
 */
async function readPayload(request: Request): Promise<string | null> {
  const contentType = request.headers.get("content-type") ?? "";
  if (contentType.includes("multipart/form-data") || contentType.includes("application/x-www-form-urlencoded")) {
    const form = await request.formData();
    const payload = form.get("payload");
    return typeof payload === "string" ? payload : null;
  }
  const text = await request.text();
  return text.trim() === "" ? null : text;
}

export function webhookRoutes(ctx: AppContext): Hono {
  const app = new Hono();

  app.post("/plex", async (c) => {
    const expected = ctx.config.webhookToken;
    if (expected) {
      const given = c.req.query("token") ?? c.req.header("x-webhook-token") ?? "";
      if (given !== expected) return c.json({ error: "invalid token" }, 401);
    }

    const text = await readPayload(c.req.raw);
    if (!text) return c.json({ error: "missing payload" }, 400);
    let raw: unknown;
    try {
      raw = JSON.parse(text);
    } catch {
      return c.json({ error: "payload is not JSON" }, 400);
    }

    const parsed = parsePlexPayload(raw);
    const logged = text.length > MAX_LOGGED_PAYLOAD ? text.slice(0, MAX_LOGGED_PAYLOAD) : text;
    if (!parsed.ok) {
      await ctx.library.logWebhook({
        event: parsed.event,
        account: null,
        player: null,
        mediaType: parsed.mediaType,
        title: parsed.title,
        outcome: `ignored: ${parsed.reason}`,
        payload: logged,
      });
      return c.json({ ok: true, ignored: parsed.reason });
    }

    const result = await ctx.scrobbler.apply(parsed.event);
    await ctx.library.logWebhook({
      event: parsed.event.event,
      account: parsed.event.account,
      player: parsed.event.player,
      mediaType: parsed.event.media.type,
      title: result.title,
      outcome: result.action,
      payload: logged,
    });
    await ctx.library.pruneWebhookLog(ctx.config.webhookRetentionDays);
    ctx.log(`${parsed.event.event} ${result.action} "${result.title}"${result.percent !== undefined ? ` ${result.percent}%` : ""}`);
    return c.json({ ok: true, ...result });
  });

  return app;
}
