<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type NxpEvent, type DashboardStats } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';
  import StatCard from '$lib/components/StatCard.svelte';

  let stats = $state<DashboardStats | null>(null);
  let events = $state<NxpEvent[]>([]);
  let liveCount = $state(0);
  let pingResult = $state<string | null>(null);
  let pingLoading = $state(false);
  let eventName = $state('');
  let eventPayload = $state('');
  let publishLoading = $state(false);
  let publishResult = $state<string | null>(null);
  let sseSubscription: { close: () => void } | null = null;

  async function loadStats() {
    try {
      stats = await api.getDashboardStats();
      const eventsResp = await api.replayEvents(0);
      events = eventsResp.events.slice().reverse().slice(0, 10);
    } catch (err) {
      console.error('Failed to load stats:', err);
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
      await loadStats();
    } catch (err) {
      publishResult = `error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      publishLoading = false;
    }
  }

  function startLiveStream() {
    sseSubscription = api.subscribeLiveEvents((evt) => {
      events = [evt, ...events].slice(0, 10);
      liveCount++;
    });
  }

  function formatAmount(minor: number, currency: string): string {
    return `${(minor / 100).toFixed(2)} ${currency}`;
  }

  function healthBadge(health: string): 'success' | 'warning' | 'error' | 'muted' {
    switch (health) {
      case 'healthy': return 'success';
      case 'degraded': return 'warning';
      case 'unhealthy': return 'error';
      default: return 'muted';
    }
  }

  onMount(() => {
    loadStats();
    startLiveStream();
    // Refresh stats every 30 seconds.
    const interval = setInterval(loadStats, 30_000);
    return () => clearInterval(interval);
  });

  onDestroy(() => {
    sseSubscription?.close();
  });
</script>

<Layout>
  <div class="mb-8">
    <h1 class="text-2xl font-semibold mb-1">Dashboard</h1>
    <p class="text-sm text-nexora-muted">
      Live view of Nexora Core · {liveCount > 0 ? `${liveCount} live events · ` : ''}Press
      <kbd class="px-1.5 py-0.5 rounded border border-nexora-border text-[10px] font-mono">⌘K</kbd> for commands
    </p>
  </div>

  <!-- Core stats -->
  {#if stats}
    <h2 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Core</h2>
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
      <StatCard label="Modules" value={stats.core.modules} badge={stats.core.enabled_modules > 0 ? 'success' : 'muted'} />
      <StatCard label="Events" value={stats.core.events_published} icon="↻" />
      <StatCard label="Principals" value={stats.core.principals} />
      <StatCard label="Health" value={stats.core.health} badge={healthBadge(stats.core.health)} />
    </div>

    <!-- Service stats grid -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-8">
      <!-- Marketplace -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Marketplace</h3>
        <div class="space-y-2">
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Total Packages</span>
            <span class="font-mono font-semibold">{stats.marketplace.total_packages}</span>
          </div>
        </div>
      </div>

      <!-- Billing -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Billing</h3>
        <div class="space-y-2">
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Revenue</span>
            <span class="font-mono font-semibold text-emerald-400">{formatAmount(stats.billing.revenue_minor, stats.billing.currency || 'USD')}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Outstanding</span>
            <span class="font-mono font-semibold text-amber-400">{formatAmount(stats.billing.outstanding_minor, stats.billing.currency || 'USD')}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Invoices</span>
            <span class="font-mono">{stats.billing.invoice_count}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Subscriptions</span>
            <span class="font-mono">{stats.billing.subscription_count}</span>
          </div>
        </div>
      </div>

      <!-- Workflow -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Workflow</h3>
        <div class="space-y-2">
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Workflows</span>
            <span class="font-mono">{stats.workflow.workflow_count}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Executions</span>
            <span class="font-mono">{stats.workflow.execution_count}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Succeeded</span>
            <span class="font-mono text-emerald-400">{stats.workflow.succeeded}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Failed</span>
            <span class="font-mono text-red-400">{stats.workflow.failed}</span>
          </div>
        </div>
      </div>

      <!-- Cluster -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Cluster</h3>
        <div class="space-y-2">
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Total Nodes</span>
            <span class="font-mono">{stats.cluster.total_nodes}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Healthy</span>
            <span class="font-mono text-emerald-400">{stats.cluster.healthy_nodes}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Unhealthy</span>
            <span class="font-mono text-red-400">{stats.cluster.unhealthy_nodes}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Offline</span>
            <span class="font-mono text-nexora-muted">{stats.cluster.offline_nodes}</span>
          </div>
        </div>
      </div>

      <!-- Notifications -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Notifications</h3>
        <div class="space-y-2">
          <div class="flex justify-between text-sm">
            <span class="text-nexora-muted">Total</span>
            <span class="font-mono font-semibold">{stats.notifications.total}</span>
          </div>
        </div>
      </div>

      <!-- Ping panel -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Core PING</h3>
        <button class="btn-primary w-full" onclick={handlePing} disabled={pingLoading}>
          {pingLoading ? 'Pinging…' : 'Send PING'}
        </button>
        {#if pingResult}
          <div class="mt-3 p-2 rounded-md bg-nexora-bg border border-nexora-border font-mono text-xs">
            {pingResult}
          </div>
        {/if}
      </div>
    </div>

    <!-- Publish event -->
    <div class="card mb-6">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Publish Event</h3>
      <form onsubmit={handlePublish} class="flex gap-2">
        <input class="input" placeholder="event name" bind:value={eventName} disabled={publishLoading} />
        <input class="input" placeholder="payload" bind:value={eventPayload} disabled={publishLoading} />
        <button type="submit" class="btn-primary" disabled={publishLoading || !eventName}>
          {publishLoading ? '…' : 'Publish'}
        </button>
      </form>
      {#if publishResult}
        <div class="mt-2 p-2 rounded-md bg-nexora-bg border border-nexora-border font-mono text-xs">{publishResult}</div>
      {/if}
    </div>
  {:else}
    <div class="card text-center text-nexora-muted">Loading stats…</div>
  {/if}

  <!-- Recent events (LIVE) -->
  <div class="card">
    <div class="flex items-center justify-between mb-3">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted flex items-center gap-2">
        Recent Events
        {#if liveCount > 0}
          <span class="relative flex h-2 w-2">
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
        {/if}
      </h3>
      <a href="/events" class="text-xs text-nexora-accent hover:underline">View all →</a>
    </div>
    {#if events.length === 0}
      <p class="text-sm text-nexora-muted">No events yet.</p>
    {:else}
      <div class="space-y-1 font-mono text-xs">
        {#each events as evt}
          <div class="flex items-center gap-3 p-2 rounded bg-nexora-bg border border-nexora-border">
            <span class="text-nexora-muted">#{evt.id}</span>
            <span class="text-nexora-accent">{evt.name}</span>
            <span class="text-nexora-text flex-1 truncate">{evt.payload}</span>
            <span class="text-nexora-muted">{new Date(evt.timestamp / 1_000_000).toLocaleTimeString()}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Layout>
