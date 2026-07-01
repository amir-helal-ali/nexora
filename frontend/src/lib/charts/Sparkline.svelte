<script lang="ts">
  // Sparkline — small inline line chart for trends.
  let {
    data = [],
    width = 200,
    height = 40,
    color = '#3b82f6',
    fill = true,
  }: {
    data: number[];
    width?: number;
    height?: number;
    color?: string;
    fill?: boolean;
  } = $props();

  let max = $derived(Math.max(...data, 1));
  let min = $derived(Math.min(...data, 0));
  let range = $derived(max - min || 1);

  let points = $derived(
    data
      .map((v, i) => {
        const x = (i / Math.max(data.length - 1, 1)) * width;
        const y = height - ((v - min) / range) * height;
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(' '),
  );

  let areaPath = $derived(
    data.length > 0
      ? `M 0,${height} L ${data
          .map((v, i) => {
            const x = (i / Math.max(data.length - 1, 1)) * width;
            const y = height - ((v - min) / range) * height;
            return `${x.toFixed(1)},${y.toFixed(1)}`;
          })
          .join(' L ')} L ${width},${height} Z`
      : '',
  );
</script>

<svg {width} {height} class="inline-block">
  {#if fill && data.length > 1}
    <path d={areaPath} fill={color} fill-opacity="0.15" />
  {/if}
  {#if data.length > 1}
    <polyline points={points} fill="none" stroke={color} stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
  {/if}
  {#if data.length > 0}
    {@const last = data[data.length - 1]}
    {@const lx = width}
    {@const ly = height - ((last - min) / range) * height}
    <circle cx={lx} cy={ly} r="2" fill={color} />
  {/if}
</svg>
