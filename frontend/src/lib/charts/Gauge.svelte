<script lang="ts">
  // Gauge — circular progress for success rate / health score.
  let {
    value = 0,
    max = 100,
    size = 100,
    label = '',
    color = '#10b981',
  }: {
    value: number;
    max?: number;
    size?: number;
    label?: string;
    color?: string;
  } = $props();

  let pct = $derived(Math.min(value / max, 1));
  let radius = $derived(size / 2 - 8);
  let circ = $derived(2 * Math.PI * radius);
  let dash = $derived(pct * circ * 0.75); // 270-degree arc
  let displayPct = $derived(Math.round(pct * 100));
</script>

<div class="flex flex-col items-center gap-1">
  <svg width={size} height={size} viewBox="0 0 {size} {size}">
    <g transform="translate({size / 2}, {size / 2})">
      <!-- Background arc (270 degrees) -->
      <circle
        r={radius}
        fill="none"
        stroke="#1f1f23"
        stroke-width="6"
        stroke-dasharray="{circ * 0.75} {circ * 0.25}"
        stroke-dashoffset="{circ * 0.125}"
        transform="rotate(135)"
      />
      <!-- Value arc -->
      <circle
        r={radius}
        fill="none"
        stroke={color}
        stroke-width="6"
        stroke-dasharray="{dash} {circ}"
        stroke-dashoffset="{circ * 0.125}"
        stroke-linecap="round"
        transform="rotate(135)"
      />
      <text text-anchor="middle" dominant-baseline="central" class="fill-nexora-text font-bold" font-size="16">
        {displayPct}%
      </text>
    </g>
  </svg>
  {#if label}
    <span class="text-xs text-nexora-muted">{label}</span>
  {/if}
</div>
