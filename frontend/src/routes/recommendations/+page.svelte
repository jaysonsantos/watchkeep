<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { ApiError, addFromCatalog } from "$lib/api.ts";
  import Poster from "$lib/components/Poster.svelte";
  import { fmtShare, languageName } from "$lib/format.ts";
  import { RECOMMEND_INPUTS, recommendationsUrl } from "$lib/lists.ts";
  import type { CatalogMovie, CatalogShow, MediaKind, Recommendation } from "$lib/types.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();

  const fromRatings = $derived(data.input === "ratings");

  /** A list of the page. `kind` is what an add button creates; `linked` lists hold library rows. */
  interface Section {
    key: string;
    kind: MediaKind;
    heading: string;
    note: string;
    linked: boolean;
    items: Recommendation<CatalogMovie | CatalogShow>[];
  }

  const sections = $derived<Section[]>(
    data.recommendations === null
      ? []
      : [
          {
            key: "movies",
            kind: "movie" as const,
            heading: "Movies for you",
            note: fromRatings ? "Closest to the genres you rate." : "Closest to the genres you watch.",
            linked: false,
            items: data.recommendations.movies,
          },
          {
            key: "shows",
            kind: "show" as const,
            heading: "Shows for you",
            note: fromRatings ? "Closest to the genres you rate." : "Closest to the genres you watch.",
            linked: false,
            items: data.recommendations.shows,
          },
          {
            key: "collection",
            kind: "movie" as const,
            heading: "Next in a collection",
            note: fromRatings ? "You rated a movie of these collections." : "You watched a part of these collections.",
            linked: false,
            items: data.recommendations.nextInCollection,
          },
          {
            key: "returning",
            kind: "show" as const,
            heading: "Returning shows",
            note: fromRatings
              ? "You finished and rated these and they still make episodes."
              : "You finished these and they still make episodes.",
            linked: true,
            items: data.recommendations.returning,
          },
        ].filter((section) => section.items.length > 0),
  );

  const profile = $derived(data.recommendations?.profile ?? null);

  let busy = $state<number | null>(null);

  async function add(kind: MediaKind, tmdbId: number, watchlist: boolean) {
    busy = tmdbId;
    try {
      await addFromCatalog(kind, tmdbId, watchlist);
      await invalidateAll();
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : String(cause));
    } finally {
      busy = null;
    }
  }
</script>

<svelte:head><title>Recommended · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Recommended</h1>
    <p class="sub">
      {#if profile && profile.items > 0}
        From {profile.items}
        {fromRatings ? "rated" : "watched"}
        {profile.items === 1 ? "item" : "items"} in your library.
      {:else if fromRatings}
        What to watch next, from your ratings.
      {:else}
        What to watch next, from your watch history.
      {/if}
    </p>
  </div>
  {#if data.recommendations !== null}
    <div class="seg">
      {#each RECOMMEND_INPUTS as [value, label] (value)}
        <a href={recommendationsUrl(value)} class:active={data.input === value}>{label}</a>
      {/each}
    </div>
  {/if}
</div>

{#if data.recommendations === null}
  <div class="notice">No TMDB catalog is configured, so there is nothing to recommend.</div>
{:else if profile && profile.items === 0}
  <div class="empty">
    {#if fromRatings}
      Nothing is rated yet. Rate a movie or a show. Then this page fills up.
    {:else}
      Nothing is watched yet. Play something, run a Plex sync, or import a history. Then this page fills up.
    {/if}
  </div>
{:else}
  {#if profile && (profile.genres.length > 0 || profile.languages.length > 0)}
    <div class="taste">
      {#if profile.genres.length > 0}
        <div class="row">
          <span class="label">Your genres</span>
          {#each profile.genres as genre (genre.name)}
            <span class="badge soft">{genre.name} <span class="muted">{fmtShare(genre.share)}%</span></span>
          {/each}
        </div>
      {/if}
      {#if profile.languages.length > 0}
        <div class="row">
          <span class="label">Your languages</span>
          {#each profile.languages as language (language.name)}
            <span class="badge soft">
              {languageName(language.name)} <span class="muted">{fmtShare(language.share)}%</span>
            </span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if sections.length === 0}
    <div class="empty">No match in the catalog yet. Watch more, or check that the catalog is filled.</div>
  {/if}

  {#each sections as section (section.key)}
    <h2>{section.heading} <span class="count">{section.items.length}</span></h2>
    <p class="sub">{section.note}</p>
    <div class="grid">
      {#each section.items as item (item.tmdbId)}
        <div class="card">
          <Poster images={data.images} path={item.posterPath} title={item.title}>
            {#snippet badge()}
              <span class="badge ok">{fmtShare(item.score)}%</span>
            {/snippet}
          </Poster>
          <div class="body">
            {#if section.linked && item.localId !== null}
              <a class="t" href="/shows/{item.localId}">{item.title}</a>
            {:else}
              <span class="t">{item.title}</span>
            {/if}
            <span class="m">{item.year ?? ""}</span>
            {#if item.reasons.length > 0}
              <span class="reasons">
                {#each item.reasons as reason (reason)}<span class="badge soft">{reason}</span>{/each}
              </span>
            {/if}
            {#if !section.linked}
              <div class="actions">
                <button
                  type="button"
                  class="primary small"
                  disabled={busy === item.tmdbId}
                  onclick={() => add(section.kind, item.tmdbId, true)}>+ List</button
                >
                <button
                  type="button"
                  class="small"
                  disabled={busy === item.tmdbId}
                  onclick={() => add(section.kind, item.tmdbId, false)}>Add</button
                >
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/each}
{/if}

<style>
  .taste {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 1.2rem;
  }
  .taste .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }
  .taste .label {
    color: var(--muted);
    font-size: 0.85rem;
    min-width: 7.5rem;
  }
  .reasons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
</style>
