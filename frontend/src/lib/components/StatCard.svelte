<script lang="ts">
  let {
    label,
    value,
    badge = null,
    icon = null,
  }: {
    label: string;
    value: string | number;
    badge?: 'success' | 'warning' | 'error' | 'muted' | null;
    icon?: string | null;
  } = $props();
</script>

<div class="card">
  <div class="flex items-center justify-between mb-2">
    <span class="text-xs uppercase tracking-wider text-nexora-muted">{label}</span>
    {#if icon}
      <span class="text-nexora-muted text-sm">{icon}</span>
    {/if}
  </div>
  <div class="flex items-baseline gap-2">
    <span class="text-2xl font-semibold">{value}</span>
    {#if badge}
      <span class="badge-{badge}">{badge}</span>
    {/if}
  </div>
</div>
