<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type NxpEvent, type Module, type HealthSnapshot } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';
  import StatCard from '$lib/components/StatCard.svelte';

  let pingResult = $state<string | null>(null);
  let pingLoading = $state(false);
  let events = $state<NxpEvent[]>([]);
  let modules = $state<Module[]>([]);
  let health = $state<HealthSnapshot | null>(null);
  let eventName = $state('');
  let eventPayload = $state('');
  let publishLoading = $state(false);
  let publishResult = $state<string | null>(null);
  let loadError = $state<string | null>(null);
  let liveCount = $state(0);
  let sseSubscription: { close: () => void } | null = null;

  async function loadAll() {
    try {
      const [eventsResp, modulesResp, healthResp] = await Promise.all([
        api.replayEvents(0),
        api.listModules(),
        api.getHealth(),
      ]);
      events = eventsResp.events.slice().reverse().slice(0, 10);
      modules = modulesResp.modules;
      health = healthResp;
    } catch (err) {
      loadError = err instanceof Error ? err.message : 'Load failed';
    }
  }

  async function handlePing() {
    pingLoading = true;
    pingResult = null;
    try {
      const start = performance.now();
      const resp = await api.ping();
      const elapsed = (performance.now() - start).toFixed(1);
      pingResult = `pong=${resp.pong} · ${elapsed}ms`;
    } catch (err) {
      pingResult = `error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      pingLoading = false;
    }
  }

  async function handlePublish(e: Event) {
    e.preventDefault();
    if (!eventName) return;
    publishLoading = true;
    publishResult = null;
    try {
      const resp = await api.publishEvent(eventName, eventPayload || '');
      publishResult = `published event_id=${resp.event_id}`;
      eventName = '';
      eventPayload = '';
    } catch (err) {
      publishResult = `error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      publishLoading = false;
    }
  }

  function startLiveStream() {
    sseSubscription = api.subscribeLiveEvents((evt) => {
      // Prepend new event to the list (newest first)
      events = [evt, ...events].slice(0, 10);
      liveCount++;
    });
  }

  onMount(() => {
    loadAll();
    startLiveStream();
  });

  onDestroy(() => {
    sseSubscription?.close();
  });
</script>

<Layout>
  <div class="mb-8">
    <h1 class="text-2xl font-semibold mb-1">Dashboard</h1>
    <p class="text-sm text-nexora-muted">
      Live view of Nexora Core · {events.length} recent events · {modules.length} modules
      {#if liveCount > 0}
        · <span class="text-emerald-400">{liveCount} live</span>
      {/if}
      · Press <kbd class="px-1.5 py-0.5 rounded border border-nexora-border text-[10px] font-mono">⌘K</kbd> for commands
    </p>
  </div>

  {#if loadError}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">
      {loadError}
    </div>
  {/if}

  <!-- Live indicator -->
  {#if liveCount > 0}
    <div class="mb-6 flex items-center gap-2 text-xs text-emerald-400">
      <span class="relative flex h-2 w-2">
        <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
        <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
      </span>
      Live · {liveCount} event{liveCount === 1 ? '' : 's'} received this session
    </div>
  {/if}

  <!-- Stats grid -->
  <div class="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
    <StatCard
      label="Modules"
      value={modules.length}
      badge={modules.length > 0 ? 'success' : 'muted'}
    />
    <StatCard
      label="Events"
      value={events.length > 0 ? events[0].id : 0}
      icon="↻"
    />
    <StatCard
      label="Live"
      value={liveCount}
      badge={liveCount > 0 ? 'success' : 'muted'}
    />
    <StatCard
      label="Health"
      value={health?.overall ?? '—'}
      badge={health?.overall === 'healthy' ? 'success' : health?.overall === 'degraded' ? 'warning' : 'error'}
    />
  </div>

  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <!-- Ping panel -->
    <div class="card">
      <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-3">
        Core PING
      </h2>
      <p class="text-xs text-nexora-muted mb-4">
        Sends an NXP PING frame through the gateway → Core → back. Verifies the full
        stack is wired correctly.
      </p>
      <button class="btn-primary" onclick={handlePing} disabled={pingLoading}>
        {pingLoading ? 'Pinging…' : 'Send PING'}
      </button>
      {#if pingResult}
        <div class="mt-4 p-3 rounded-md bg-nexora-bg border border-nexora-border font-mono text-sm">
          {pingResult}
        </div>
      {/if}
    </div>

    <!-- Publish event panel -->
    <div class="card">
      <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-3">
        Publish Event
      </h2>
      <form onsubmit={handlePublish} class="space-y-3">
        <input
          class="input"
          placeholder="event name (e.g. project.created)"
          bind:value={eventName}
          disabled={publishLoading}
        />
        <input
          class="input"
          placeholder="payload (optional)"
          bind:value={eventPayload}
          disabled={publishLoading}
        />
        <button type="submit" class="btn-primary w-full" disabled={publishLoading || !eventName}>
          {publishLoading ? 'Publishing…' : 'Publish'}
        </button>
      </form>
      {#if publishResult}
        <div class="mt-3 p-3 rounded-md bg-nexora-bg border border-nexora-border font-mono text-sm">
          {publishResult}
        </div>
      {/if}
      <p class="mt-3 text-xs text-nexora-muted">
        Published events appear instantly in the list below via SSE.
      </p>
    </div>

    <!-- Recent events (LIVE) -->
    <div class="card lg:col-span-2">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted flex items-center gap-2">
          Recent Events
          <span class="relative flex h-2 w-2">
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
        </h2>
        <a href="/events" class="text-xs text-nexora-accent hover:underline">View all →</a>
      </div>
      {#if events.length === 0}
        <p class="text-sm text-nexora-muted">No events yet. Publish one above.</p>
      {:else}
        <div class="space-y-2 font-mono text-xs">
          {#each events as evt}
            <div class="flex items-center gap-3 p-2 rounded bg-nexora-bg border border-nexora-border transition-colors hover:border-nexora-accent/50">
              <span class="text-nexora-muted">#{evt.id}</span>
              <span class="text-nexora-accent">{evt.name}</span>
              <span class="text-nexora-text flex-1 truncate">{evt.payload}</span>
              <span class="text-nexora-muted">
                {new Date(evt.timestamp / 1_000_000).toLocaleTimeString()}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</Layout>
