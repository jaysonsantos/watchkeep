<script lang="ts">
  /** A column chart of counts. Every bar is a share of the tallest one. */
  interface Bar {
    /** The text under the bar. */
    label: string;
    value: number;
    /** The tooltip of the bar. */
    hint: string;
    /** Marks the bar that stands out, for example the busiest hour. */
    peak?: boolean;
  }

  /** Bars keep a visible stub when the value is zero. */
  const MIN_HEIGHT_PERCENT = 2;
  const FULL_PERCENT = 100;

  let { bars, labelEvery = 1 }: { bars: Bar[]; labelEvery?: number } = $props();

  const top = $derived(Math.max(...bars.map((bar) => bar.value), 1));

  function height(value: number): number {
    return value === 0 ? MIN_HEIGHT_PERCENT : Math.max(MIN_HEIGHT_PERCENT, (value / top) * FULL_PERCENT);
  }
</script>

<div class="chart">
  {#each bars as bar, index (bar.label)}
    <div class="col" title={bar.hint}>
      <div class="stem"><span class:peak={bar.peak} style:height="{height(bar.value)}%"></span></div>
      <div class="tick">{index % labelEvery === 0 ? bar.label : ""}</div>
    </div>
  {/each}
</div>
