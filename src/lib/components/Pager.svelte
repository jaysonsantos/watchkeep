<script lang="ts">
  interface Props {
    page: number;
    pages: number;
    href: (page: number) => string;
    previous?: string;
    next?: string;
  }

  let { page, pages, href, previous = "← Previous", next = "Next →" }: Props = $props();
</script>

<div class="pager">
  {#if page > 1}<a class="btn" href={href(page - 1)}>{previous}</a>{/if}
  <span class="muted">Page {page} of {pages}</span>
  {#if page < pages}<a class="btn" href={href(page + 1)}>{next}</a>{/if}
</div>
