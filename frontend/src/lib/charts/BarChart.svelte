<script lang="ts">
  // Bar chart — for categorical data.
  let {
    data = [],
    width = 300,
    height = 120,
    barColor = '#3b82f6',
  }: {
    data: Array<{ label: string; value: number }>;
    width?: number;
    height?: number;
    barColor?: string;
  } = $props();

  let max = $derived(Math.max(...data.map((d) => d.value), 1));
  let barWidth = $derived(data.length > 0 ? (width - 20) / data.length - 4 : 0);
</script>

<svg {width} {height} class="block">
  {#each data as d, i}
    {@const bh = (d.value / max) * (height - 25)}
    {@const bx = 10 + i * (barWidth + 4)}
    {@const by = height - 20 - bh}
    <rect x={bx} y={by} width={barWidth} height={bh} rx="2" fill={barColor} fill-opacity="0.8" />
    <text x={bx + barWidth / 2} y={height - 8} text-anchor="middle" class="fill-nexora-muted" font-size="9">
      {d.label.length > 8 ? d.label.slice(0, 7) + '…' : d.label}
    </text>
    <text x={bx + barWidth / 2} y={by - 3} text-anchor="middle" class="fill-nexora-text" font-size="9" font-weight="bold">
      {d.value}
    </text>
  {/each}
</svg>
