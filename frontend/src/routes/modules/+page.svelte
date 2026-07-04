<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Module } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let modules = $state<Module[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      const resp = await api.listModules();
      modules = resp.modules;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  function stateBadge(state: Module['state']) {
    switch (state) {
      case 'enabled': return 'success';
      case 'paused': return 'warning';
      case 'removed': return 'error';
      default: return 'muted';
    }
  }

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Modules</h1>
      <p class="text-sm text-nexora-muted">{modules.length} modules registered</p>
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

  {#if modules.length === 0 && !loading}
    <div class="card text-center text-nexora-muted">
      No modules installed yet. Use the Core's ExecuteCommand to install one.
    </div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {#each modules as m}
        <div class="card">
          <div class="flex items-start justify-between mb-3">
            <div>
              <h3 class="font-semibold">{m.name}</h3>
              <p class="text-xs text-nexora-muted font-mono">{m.id}</p>
            </div>
            <span class="badge-{stateBadge(m.state)}">{m.state}</span>
          </div>
          <dl class="space-y-2 text-xs">
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Version</dt>
              <dd class="font-mono">{m.version}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Owner</dt>
              <dd>{m.owner}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Transitions</dt>
              <dd class="font-mono">{m.transition_count}</dd>
            </div>
            <div>
              <dt class="text-nexora-muted mb-1">Capabilities</dt>
              <dd class="flex flex-wrap gap-1">
                {#each m.capabilities as cap}
                  <span class="badge-muted">{cap}</span>
                {/each}
              </dd>
            </div>
          </dl>
        </div>
      {/each}
    </div>
  {/if}
</Layout>
