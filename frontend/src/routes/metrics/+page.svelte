<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type DashboardStats, type NxpEvent } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';
  import Sparkline from '$lib/charts/Sparkline.svelte';
  import Donut from '$lib/charts/Donut.svelte';
  import BarChart from '$lib/charts/BarChart.svelte';
  import Gauge from '$lib/charts/Gauge.svelte';

  let stats = $state<DashboardStats | null>(null);
  let events = $state<NxpEvent[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let sseSub: { close: () => void } | null = null;

  // Time-series data (updated on each refresh).
  let eventSeries = $state<number[]>([]);
  let revenueSeries = $state<number[]>([]);
  let maxEventSeries = 20;
  let maxRevenueSeries = 20;

  async function load() {
    try {
      const [statsResp, eventsResp] = await Promise.all([
        api.getDashboardStats(),
        api.replayEvents(0),
      ]);
      stats = statsResp;
      events = eventsResp.events;

      // Push to time series.
      eventSeries = [...eventSeries, eventsResp.events.length].slice(-maxEventSeries);
      if (statsResp.billing) {
        revenueSeries = [...revenueSeries, statsResp.billing.revenue_minor].slice(-maxRevenueSeries);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  // Event name distribution for bar chart.
  let eventDistribution = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const evt of events) {
      const name = evt.name.split('.')[0]; // Group by prefix
      counts[name] = (counts[name] || 0) + 1;
    }
    return Object.entries(counts)
      .map(([label, value]) => ({ label, value }))
      .sort((a, b) => b.value - a.value)
      .slice(0, 8);
  });

  // Cluster health donut data.
  let clusterData = $derived.by(() => {
    if (!stats) return [];
    const c = stats.cluster;
    return [
      { label: 'Healthy', value: c.healthy_nodes, color: '#10b981' },
      { label: 'Degraded', value: c.degraded_nodes, color: '#f59e0b' },
      { label: 'Unhealthy', value: c.unhealthy_nodes, color: '#ef4444' },
      { label: 'Offline', value: c.offline_nodes, color: '#71717a' },
    ].filter((d) => d.value > 0);
  });

  // Workflow success gauge.
  let workflowPct = $derived.by(() => {
    if (!stats || stats.workflow.execution_count === 0) return 0;
    return (stats.workflow.succeeded / stats.workflow.execution_count) * 100;
  });

  // Billing donut data.
  let billingData = $derived.by(() => {
    if (!stats) return [];
    const b = stats.billing;
    return [
      { label: 'Revenue', value: b.revenue_minor, color: '#10b981' },
      { label: 'Outstanding', value: b.outstanding_minor, color: '#f59e0b' },
    ].filter((d) => d.value > 0);
  });

  function formatAmount(minor: number, currency: string): string {
    return `${(minor / 100).toFixed(2)} ${currency}`;
  }

  onMount(() => {
    load();
    const interval = setInterval(load, 15_000);
    // Live updates via SSE — push to event series.
    sseSub = api.subscribeLiveEvents((evt) => {
      eventSeries = [...eventSeries, eventSeries[eventSeries.length - 1] + 1].slice(-maxEventSeries);
    });
    return () => {
      clearInterval(interval);
      sseSub?.close();
    };
  });

  onDestroy(() => {
    sseSub?.close();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Metrics & Analytics</h1>
      <p class="text-sm text-nexora-muted">Real-time visual analytics · auto-refresh every 15s</p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>
      {loading ? 'Loading…' : '↻ Refresh'}
    </button>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">{error}</div>
  {/if}

  {#if stats}
    <!-- Row 1: Time-series sparklines -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6 mb-6">
      <div class="card">
        <div class="flex items-center justify-between mb-3">
          <div>
            <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted">Event Activity</h3>
            <p class="text-2xl font-semibold mt-1">{stats.core.events_published} <span class="text-sm text-nexora-muted">total events</span></p>
          </div>
        </div>
        {#if eventSeries.length > 1}
          <Sparkline data={eventSeries} width={280} height={50} color="#3b82f6" />
          <p class="text-xs text-nexora-muted mt-1">Last {eventSeries.length} samples</p>
        {:else}
          <p class="text-sm text-nexora-muted py-8 text-center">Collecting data…</p>
        {/if}
      </div>

      <div class="card">
        <div class="flex items-center justify-between mb-3">
          <div>
            <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted">Revenue Trend</h3>
            <p class="text-2xl font-semibold mt-1 text-emerald-400">
              {formatAmount(stats.billing.revenue_minor, stats.billing.currency || 'USD')}
            </p>
          </div>
        </div>
        {#if revenueSeries.length > 1}
          <Sparkline data={revenueSeries} width={280} height={50} color="#10b981" />
          <p class="text-xs text-nexora-muted mt-1">Last {revenueSeries.length} samples</p>
        {:else}
          <p class="text-sm text-nexora-muted py-8 text-center">Collecting data…</p>
        {/if}
      </div>
    </div>

    <!-- Row 2: Donut charts + Gauge -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6">
      <!-- Cluster health donut -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Cluster Health</h3>
        {#if clusterData.length > 0}
          <Donut data={clusterData} size={120} thickness={16} />
        {:else}
          <p class="text-sm text-nexora-muted py-8 text-center">No nodes registered</p>
        {/if}
      </div>

      <!-- Billing donut -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Billing Breakdown</h3>
        {#if billingData.length > 0}
          <Donut data={billingData} size={120} thickness={16} />
          <div class="mt-3 space-y-1 text-xs">
            <div class="flex justify-between">
              <span class="text-nexora-muted">Revenue</span>
              <span class="font-mono text-emerald-400">{formatAmount(stats.billing.revenue_minor, stats.billing.currency || 'USD')}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-nexora-muted">Outstanding</span>
              <span class="font-mono text-amber-400">{formatAmount(stats.billing.outstanding_minor, stats.billing.currency || 'USD')}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-nexora-muted">Invoices</span>
              <span class="font-mono">{stats.billing.invoice_count}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-nexora-muted">Subscriptions</span>
              <span class="font-mono">{stats.billing.subscription_count}</span>
            </div>
          </div>
        {:else}
          <p class="text-sm text-nexora-muted py-8 text-center">No billing data</p>
        {/if}
      </div>

      <!-- Workflow success gauge -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Workflow Success Rate</h3>
        <div class="flex flex-col items-center justify-center py-4">
          <Gauge
            value={workflowPct}
            max={100}
            size={120}
            label={`${stats.workflow.execution_count} executions`}
            color={workflowPct >= 80 ? '#10b981' : workflowPct >= 50 ? '#f59e0b' : '#ef4444'}
          />
          <div class="mt-3 grid grid-cols-3 gap-2 text-xs w-full">
            <div class="text-center">
              <p class="text-nexora-muted">Succeeded</p>
              <p class="font-mono text-emerald-400 font-semibold">{stats.workflow.succeeded}</p>
            </div>
            <div class="text-center">
              <p class="text-nexora-muted">Failed</p>
              <p class="font-mono text-red-400 font-semibold">{stats.workflow.failed}</p>
            </div>
            <div class="text-center">
              <p class="text-nexora-muted">Stopped</p>
              <p class="font-mono text-amber-400 font-semibold">{stats.workflow.stopped}</p>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Row 3: Bar chart + summary stats -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <!-- Event distribution bar chart -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Event Distribution by Type</h3>
        {#if eventDistribution.length > 0}
          <BarChart data={eventDistribution} width={320} height={140} barColor="#3b82f6" />
        {:else}
          <p class="text-sm text-nexora-muted py-8 text-center">No events to display</p>
        {/if}
      </div>

      <!-- Summary stats -->
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Platform Summary</h3>
        <div class="grid grid-cols-2 gap-4">
          <div>
            <p class="text-xs text-nexora-muted">Modules</p>
            <p class="text-xl font-semibold">{stats.core.modules}</p>
          </div>
          <div>
            <p class="text-xs text-nexora-muted">Principals</p>
            <p class="text-xl font-semibold">{stats.core.principals}</p>
          </div>
          <div>
            <p class="text-xs text-nexora-muted">Packages</p>
            <p class="text-xl font-semibold">{stats.marketplace.total_packages}</p>
          </div>
          <div>
            <p class="text-xs text-nexora-muted">Notifications</p>
            <p class="text-xl font-semibold">{stats.notifications.total}</p>
          </div>
          <div>
            <p class="text-xs text-nexora-muted">Cluster Nodes</p>
            <p class="text-xl font-semibold">{stats.cluster.total_nodes}</p>
          </div>
          <div>
            <p class="text-xs text-nexora-muted">Health</p>
            <p class="text-xl font-semibold {stats.core.health === 'healthy' ? 'text-emerald-400' : 'text-amber-400'}">{stats.core.health}</p>
          </div>
        </div>
      </div>
    </div>
  {:else if !loading}
    <div class="card text-center text-nexora-muted">Failed to load metrics.</div>
  {/if}
</Layout>
