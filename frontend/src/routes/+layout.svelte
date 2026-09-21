<script lang="ts">
  import "../app.css";
  import { page } from "$app/state";
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

  function active(href: string): boolean {
    const path = page.url.pathname;
    return href === "/" ? path === "/" : path === href || path.startsWith(`${href}/`);
  }
</script>

<header>
  <a class="brand" href="/" aria-label="Watchkeep">
    <img src="/logo.svg" alt="" width="30" height="30" /><span class="name">Watchkeep</span>
  </a>
  <nav bind:this={nav}>
    {#each links as [href, label] (href)}
      <a {href} class:active={active(href)}>{label}</a>
    {/each}
  </nav>
  <form class="quick" method="GET" action="/add">
    <input type="search" name={QUERY.search} placeholder="Search catalog…" aria-label="Search the catalog" />
  </form>
</header>
<main>{@render children()}</main>
<footer>Watchkeep · self-hosted watch history</footer>
