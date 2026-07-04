<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type NxpEvent } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let events = $state<NxpEvent[]>([]);
  let filter = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let liveCount = $state(0);
  let sseSubscription: { close: () => void } | null = null;

  async function load() {
    loading = true;
    error = null;
    try {
      const resp = await api.replayEvents(0, filter || undefined);
      events = resp.events.slice().reverse();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  function startLiveStream() {
    sseSubscription = api.subscribeLiveEvents(
      (evt) => {
        events = [evt, ...events];
        liveCount++;
      },
      filter || undefined,
    );
  }

  function restartStream() {
    sseSubscription?.close();
    liveCount = 0;
    startLiveStream();
  }

  onMount(() => {
    load();
    startLiveStream();
  });

  onDestroy(() => {
    sseSubscription?.close();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Events</h1>
      <p class="text-sm text-nexora-muted">
        Source of truth · {events.length} events
        {#if liveCount > 0}
          · <span class="text-emerald-400">{liveCount} new</span>
        {/if}
      </p>
    </div>
    <div class="flex items-center gap-2">
      {#if liveCount > 0}
        <span class="flex items-center gap-2 text-xs text-emerald-400">
          <span class="relative flex h-2 w-2">
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
          Live
        </span>
      {/if}
      <button class="btn-ghost" onclick={load} disabled={loading}>
        {loading ? 'Loading…' : '↻ Refresh'}
      </button>
    </div>
  </div>

  <div class="card mb-6">
    <form
      class="flex gap-2"
      onsubmit={(e) => {
        e.preventDefault();
        load();
        restartStream();
      }}
    >
      <input
        class="input"
        placeholder="filter by name prefix (e.g. user. or module.)"
        bind:value={filter}
      />
      <button type="submit" class="btn-primary">Filter</button>
    </form>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">
      {error}
    </div>
  {/if}

  {#if events.length === 0 && !loading}
    <div class="card text-center text-nexora-muted">No events match this filter.</div>
  {:else}
    <div class="card overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border">
            <th class="py-2 pr-4">ID</th>
            <th class="py-2 pr-4">Name</th>
            <th class="py-2 pr-4">Payload</th>
            <th class="py-2 pr-4">Timestamp</th>
          </tr>
        </thead>
        <tbody class="font-mono">
          {#each events as evt, i}
            <tr
              class="border-b border-nexora-border/50 hover:bg-nexora-bg/50
                {i === 0 && liveCount > 0 ? 'bg-emerald-500/5' : ''}"
            >
              <td class="py-2 pr-4 text-nexora-muted">#{evt.id}</td>
              <td class="py-2 pr-4 text-nexora-accent">{evt.name}</td>
              <td class="py-2 pr-4 text-nexora-text max-w-md truncate">{evt.payload}</td>
              <td class="py-2 pr-4 text-nexora-muted">
                {new Date(evt.timestamp / 1_000_000).toISOString()}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Layout>
