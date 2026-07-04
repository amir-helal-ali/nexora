<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type HealthSnapshot } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let health = $state<HealthSnapshot | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      health = await api.getHealth();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  function statusBadge(status: 'healthy' | 'degraded' | 'unhealthy') {
    switch (status) {
      case 'healthy': return 'success';
      case 'degraded': return 'warning';
      case 'unhealthy': return 'error';
    }
  }

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">System Health</h1>
      <p class="text-sm text-nexora-muted">
        Overall: {health?.overall ?? '—'}
      </p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>
      {loading ? 'Loading…' : '↻ Refresh'}
    </button>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">
      {error}
    </div>
  {/if}

  {#if health}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {#each health.subsystems as sub}
        <div class="card">
          <div class="flex items-start justify-between mb-3">
            <h3 class="font-semibold">{sub.name}</h3>
            <span class="badge-{statusBadge(sub.status)}">{sub.status}</span>
          </div>
          {#if sub.message}
            <p class="text-xs text-nexora-muted">{sub.message}</p>
          {/if}
          <p class="mt-2 text-xs text-nexora-muted font-mono">
            last check: {new Date(sub.last_check / 1_000_000).toLocaleTimeString()}
          </p>
        </div>
      {/each}

      {#if health.subsystems.length === 0}
        <div class="card col-span-full text-center text-nexora-muted">
          No subsystems registered yet. Health is reported as "healthy" by default.
        </div>
      {/if}
    </div>
  {/if}
</Layout>
