<script lang="ts">
  import "../app.css";
  import { navigating, page } from "$app/state";
  import { QUERY } from "$lib/lists.ts";

  let { children } = $props();

  const links: Array<[string, string]> = [
    ["/", "Dashboard"],
    ["/movies", "Movies"],
    ["/shows", "Shows"],
    ["/recommendations", "Recommended"],
    ["/watchlist", "Watchlist"],
    ["/history", "History"],
    ["/stats", "Statistics"],
    ["/add", "Add"],
    ["/webhooks", "Webhooks"],
  ];

  let nav: HTMLElement | undefined = $state();

  // On a phone the links are one row that scrolls sideways, so the link of the page must come into view.
  $effect(() => {
    void page.url.pathname;
    nav?.querySelector(".active")?.scrollIntoView({ inline: "center", block: "nearest" });
  });

  function matches(href: string, path: string): boolean {
    return href === "/" ? path === "/" : path === href || path.startsWith(`${href}/`);
  }

  function active(href: string): boolean {
    return matches(href, page.url.pathname);
  }

  // The link of the page that loads now. A slow database must not leave the click without an answer.
  function pending(href: string): boolean {
    const target = navigating.to?.url.pathname;
    return target !== undefined && !active(href) && matches(href, target);
  }
</script>

{#if navigating.to}<div class="loading-bar" role="progressbar" aria-label="Loading the page"></div>{/if}
<header>
  <a class="brand" href="/" aria-label="Watchkeep">
    <img src="/logo.svg" alt="" width="30" height="30" /><span class="name">Watchkeep</span>
  </a>
  <nav bind:this={nav}>
    {#each links as [href, label] (href)}
      <a {href} class:active={active(href)} class:pending={pending(href)}>{label}</a>
    {/each}
  </nav>
  <form class="quick" method="GET" action="/add">
    <input type="search" name={QUERY.search} placeholder="Search catalog…" aria-label="Search the catalog" />
  </form>
</header>
<main aria-busy={navigating.to ? "true" : undefined}>{@render children()}</main>
<footer>Watchkeep · self-hosted watch history</footer>
