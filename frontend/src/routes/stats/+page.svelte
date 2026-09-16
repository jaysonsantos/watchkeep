<script lang="ts">
  import BarChart from "$lib/components/BarChart.svelte";
  import Thumb from "$lib/components/Thumb.svelte";
  import { fmtCount, fmtDay, fmtHour, fmtHours, fmtMonth, fmtWatchTime, fmtWeekday } from "$lib/format.ts";
  import { peakBucket, sharePercent } from "$lib/stats.ts";
  import type { LabelCount, TopItem } from "$lib/types.ts";
  import type { PageProps } from "./$types";

  /** Label every third month, so that the ticks stay readable. */
  const MONTH_LABEL_EVERY = 3;

  /** Label every third hour, for the same reason. */
  const HOUR_LABEL_EVERY = 3;

  let { data }: PageProps = $props();

  const stats = $derived(data.stats);
  const totals = $derived(stats.totals);

  const tiles = $derived([
    { label: "Watch time", value: fmtWatchTime(totals.runtime_ms), hint: fmtHours(totals.runtime_ms) },
    {
      label: "Plays",
      value: fmtCount(totals.plays),
      hint: `${fmtCount(totals.movie_plays)} movie plays, ${fmtCount(totals.episode_plays)} episode plays`,
    },
    {
      label: "Movies watched",
      value: fmtCount(totals.movies_watched),
      hint: `of ${fmtCount(stats.library.movies)} in the library`,
    },
    {
      label: "Episodes watched",
      value: fmtCount(totals.episodes_watched),
      hint: `of ${fmtCount(stats.library.episodes)} known episodes`,
    },
    {
      label: "Shows watched",
      value: fmtCount(totals.shows_watched),
      hint: `of ${fmtCount(stats.library.shows)} tracked shows`,
    },
    { label: "Days with a play", value: fmtCount(totals.days_watched), hint: "Local days, from the first play on" },
    { label: "Current streak", value: `${fmtCount(stats.streak.current)}d`, hint: "Days in a row, up to today" },
    {
      label: "Longest streak",
      value: `${fmtCount(stats.streak.longest)}d`,
      hint: "The longest run of days with a play",
    },
  ]);

  const monthBars = $derived(
    stats.months.map((month) => ({
      label: fmtMonth(month.period),
      value: month.plays,
      hint: `${month.period}: ${fmtCount(month.plays)} plays · ${fmtWatchTime(month.runtime_ms)}`,
    })),
  );

  const busiestDay = $derived(peakBucket(stats.weekdays));
  const weekdayBars = $derived(
    stats.weekdays.map((day) => ({
      label: fmtWeekday(day.bucket),
      value: day.plays,
      hint: `${fmtWeekday(day.bucket)}: ${fmtCount(day.plays)} plays · ${fmtWatchTime(day.runtime_ms)}`,
      peak: day === busiestDay,
    })),
  );

  const busiestHour = $derived(peakBucket(stats.hours));
  const hourBars = $derived(
    stats.hours.map((hour) => ({
      label: fmtHour(hour.bucket),
      value: hour.plays,
      hint: `${fmtHour(hour.bucket)}:00 — ${fmtCount(hour.plays)} plays · ${fmtWatchTime(hour.runtime_ms)}`,
      peak: hour === busiestHour,
    })),
  );

  const split = $derived([
    { label: "Episodes", runtime: totals.episode_runtime_ms, plays: totals.episode_plays },
    { label: "Movies", runtime: totals.movie_runtime_ms, plays: totals.movie_plays },
  ]);

  function topLine(item: TopItem): string {
    const parts = [`${fmtCount(item.plays)} plays`, fmtWatchTime(item.runtime_ms)];
    return parts.join(" · ");
  }

  function labelShare(entry: LabelCount): number {
    return sharePercent(entry.plays, totals.plays);
  }
</script>

<svelte:head><title>Statistics · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Statistics</h1>
    <p class="sub">
      What you watched, when you watched it, and where the time went. The days and the hours are local to
      <code>{stats.timezone}</code>.
    </p>
  </div>
</div>

{#if totals.plays === 0}
  <div class="empty">No play recorded yet. Watch something, or import a Trakt export.</div>
{:else}
  <div class="tiles">
    {#each tiles as tile (tile.label)}
      <div class="tile" title={tile.hint}>
        <div class="n">{tile.value}</div>
        <div class="l">{tile.label}</div>
      </div>
    {/each}
  </div>

  <p class="muted">
    First play {fmtDay(totals.first_play_at)} · last play {fmtDay(totals.last_play_at)}
  </p>

  <h2>Where the time went</h2>
  <div class="split">
    {#each split as part (part.label)}
      <div class="split-row">
        <span class="k">{part.label}</span>
        <div class="bar"><span style:width="{sharePercent(part.runtime, totals.runtime_ms)}%"></span></div>
        <span class="v">{fmtWatchTime(part.runtime)}</span>
        <span class="muted">{fmtCount(part.plays)} plays</span>
      </div>
    {/each}
  </div>

  <h2>Plays per month</h2>
  <BarChart bars={monthBars} labelEvery={MONTH_LABEL_EVERY} />

  <div class="two-up">
    <section>
      <h2>Plays per weekday</h2>
      <BarChart bars={weekdayBars} />
      {#if busiestDay}
        <p class="muted">Your busiest day is {fmtWeekday(busiestDay.bucket)}.</p>
      {/if}
    </section>
    <section>
      <h2>Plays per hour</h2>
      <BarChart bars={hourBars} labelEvery={HOUR_LABEL_EVERY} />
      {#if busiestHour}
        <p class="muted">You start most plays around {fmtHour(busiestHour.bucket)}:00.</p>
      {/if}
    </section>
  </div>

  <div class="two-up">
    <section>
      <h2>Top shows</h2>
      {#if stats.top_shows.length === 0}
        <div class="empty">No episode play yet.</div>
      {:else}
        <div class="list">
          {#each stats.top_shows as show (show.id)}
            <div class="row">
              <Thumb images={data.images} path={show.poster_path} />
              <div class="what">
                <a href="/shows/{show.id}">{show.title}</a>
                <div class="muted">{fmtCount(show.items)} episodes · {topLine(show)}</div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
    <section>
      <h2>Top movies</h2>
      {#if stats.top_movies.length === 0}
        <div class="empty">No movie play yet.</div>
      {:else}
        <div class="list">
          {#each stats.top_movies as movie (movie.id)}
            <div class="row">
              <Thumb images={data.images} path={movie.poster_path} />
              <div class="what">
                <a href="/movies/{movie.id}">{movie.title}</a>{#if movie.year}&nbsp;<span class="muted">({movie.year})</span>{/if}
                <div class="muted">{topLine(movie)}</div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>

  <div class="two-up">
    <section>
      <h2>Sources</h2>
      <div class="split">
        {#each stats.sources as source (source.label)}
          <div class="split-row">
            <span class="k"><code>{source.label}</code></span>
            <div class="bar"><span style:width="{labelShare(source)}%"></span></div>
            <span class="v">{fmtCount(source.plays)}</span>
            <span class="muted">{labelShare(source)}%</span>
          </div>
        {/each}
      </div>
    </section>
    <section>
      <h2>Players</h2>
      {#if stats.players.length === 0}
        <div class="empty">No play carries a player name.</div>
      {:else}
        <div class="split">
          {#each stats.players as player (player.label)}
            <div class="split-row">
              <span class="k">{player.label}</span>
              <div class="bar"><span style:width="{labelShare(player)}%"></span></div>
              <span class="v">{fmtCount(player.plays)}</span>
              <span class="muted">{labelShare(player)}%</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>
{/if}
