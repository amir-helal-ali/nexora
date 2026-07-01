<script lang="ts">
  // Donut chart — for proportional data (e.g. node health distribution).
  let {
    data = [],
    size = 120,
    thickness = 16,
  }: {
    data: Array<{ label: string; value: number; color: string }>;
    size?: number;
    thickness?: number;
  } = $props();

  let total = $derived(data.reduce((s, d) => s + d.value, 0) || 1);
  let radius = $derived((size - thickness) / 2);
  let circumference = $derived(2 * Math.PI * radius);

  let segments = $derived.by(() => {
    let offset = 0;
    return data.map((d) => {
      const fraction = d.value / total;
      const dash = fraction * circumference;
      const seg = {
        ...d,
        dasharray: `${dash} ${circumference - dash}`,
        dashoffset: -offset,
        percentage: Math.round(fraction * 100),
      };
      offset += dash;
      return seg;
    });
  });
</script>

<div class="flex items-center gap-4">
  <svg width={size} height={size} viewBox="0 0 {size} {size}">
    <g transform="translate({size / 2}, {size / 2}) rotate(-90)">
      <circle r={radius} fill="none" stroke="#1f1f23" stroke-width={thickness} />
      {#each segments as seg}
        <circle
          r={radius}
          fill="none"
          stroke={seg.color}
          stroke-width={thickness}
          stroke-dasharray={seg.dasharray}
          stroke-dashoffset={seg.dashoffset}
          stroke-linecap="butt"
        />
      {/each}
    </g>
    <text x="50%" y="50%" text-anchor="middle" dominant-baseline="central" class="fill-nexora-text font-bold" font-size="18">
      {total}
    </text>
  </svg>
  <div class="space-y-1.5">
    {#each segments as seg}
      <div class="flex items-center gap-2 text-xs">
        <span class="w-3 h-3 rounded-sm" style="background:{seg.color}"></span>
        <span class="text-nexora-text">{seg.label}</span>
        <span class="text-nexora-muted font-mono ml-auto">{seg.value} ({seg.percentage}%)</span>
      </div>
    {/each}
  </div>
</div>
