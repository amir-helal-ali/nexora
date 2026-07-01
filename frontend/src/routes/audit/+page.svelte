<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type NxpEvent } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let allEvents = $state<NxpEvent[]>([]);
  let filteredEvents = $state<NxpEvent[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // Filters
  let searchText = $state('');
  let selectedCategory = $state<string>('');
  let selectedTimeRange = $state<string>('all');
  let sortBy = $state<'newest' | 'oldest'>('newest');

  // Event categories for filtering
  const categories = [
    { value: '', label: 'All Categories' },
    { value: 'user.', label: 'Users' },
    { value: 'module.', label: 'Modules' },
    { value: 'package.', label: 'Packages' },
    { value: 'invoice.', label: 'Invoices' },
    { value: 'payment.', label: 'Payments' },
    { value: 'subscription.', label: 'Subscriptions' },
    { value: 'workflow.', label: 'Workflows' },
    { value: 'cluster.', label: 'Cluster' },
    { value: 'notification.', label: 'Notifications' },
  ];

  const timeRanges = [
    { value: 'all', label: 'All Time', ms: 0 },
    { value: '1h', label: 'Last Hour', ms: 3_600_000 },
    { value: '24h', label: 'Last 24 Hours', ms: 86_400_000 },
    { value: '7d', label: 'Last 7 Days', ms: 604_800_000 },
  ];

  async function load() {
    loading = true;
    error = null;
    try {
      const resp = await api.replayEvents(0);
      allEvents = resp.events;
      applyFilters();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  function applyFilters() {
    let result = [...allEvents];

    // Category filter
    if (selectedCategory) {
      result = result.filter((e) => e.name.startsWith(selectedCategory));
    }

    // Time range filter
    if (selectedTimeRange !== 'all') {
      const range = timeRanges.find((r) => r.value === selectedTimeRange);
      if (range && range.ms > 0) {
        const cutoff = Date.now() - range.ms;
        result = result.filter((e) => e.timestamp / 1_000_000 >= cutoff);
      }
    }

    // Text search (in name + payload)
    if (searchText.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(
        (e) =>
          e.name.toLowerCase().includes(q) ||
          e.payload.toLowerCase().includes(q) ||
          String(e.id).includes(q),
      );
    }

    // Sort
    if (sortBy === 'newest') {
      result.sort((a, b) => b.id - a.id);
    } else {
      result.sort((a, b) => a.id - b.id);
    }

    filteredEvents = result;
  }

  // Re-apply filters when any filter changes
  $effect(() => {
    searchText; selectedCategory; selectedTimeRange; sortBy;
    applyFilters();
  });

  function categoryOf(name: string): string {
    const prefix = name.split('.')[0];
    return prefix;
  }

  function categoryColor(name: string): string {
    const cat = categoryOf(name);
    const map: Record<string, string> = {
      user: 'text-blue-400',
      module: 'text-emerald-400',
      package: 'text-purple-400',
      invoice: 'text-amber-400',
      payment: 'text-emerald-400',
      subscription: 'text-cyan-400',
      workflow: 'text-orange-400',
      cluster: 'text-pink-400',
      notification: 'text-indigo-400',
    };
    return map[cat] || 'text-nexora-muted';
  }

  function categoryBadge(name: string): string {
    const cat = categoryOf(name);
    const map: Record<string, string> = {
      user: 'badge-muted',
      module: 'badge-success',
      package: 'badge-muted',
      invoice: 'badge-warning',
      payment: 'badge-success',
      subscription: 'badge-muted',
      workflow: 'badge-warning',
      cluster: 'badge-error',
      notification: 'badge-muted',
    };
    return map[cat] || 'badge-muted';
  }

  function formatDate(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleString();
  }

  function timeAgo(ns: number): string {
    const ms = ns / 1_000_000;
    const diff = Date.now() - ms;
    const s = Math.floor(diff / 1000);
    if (s < 60) return `${s}s ago`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  function exportJson() {
    const data = JSON.stringify(filteredEvents, null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `nexora-audit-log-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // Stats
  let stats = $derived.by(() => {
    const cats: Record<string, number> = {};
    for (const e of allEvents) {
      const c = categoryOf(e.name);
      cats[c] = (cats[c] || 0) + 1;
    }
    return {
      total: allEvents.length,
      filtered: filteredEvents.length,
      byCategory: cats,
    };
  });

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Audit Log</h1>
      <p class="text-sm text-nexora-muted">
        {stats.total} total events · {stats.filtered} shown · immutable record of all platform actions
      </p>
    </div>
    <div class="flex items-center gap-2">
      <button class="btn-ghost text-xs" onclick={exportJson} disabled={filteredEvents.length === 0}>
        ⬇ Export JSON
      </button>
      <button class="btn-ghost" onclick={load} disabled={loading}>
        {loading ? 'Loading…' : '↻ Refresh'}
      </button>
    </div>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">{error}</div>
  {/if}

  <!-- Filters -->
  <div class="card mb-6">
    <div class="grid grid-cols-1 md:grid-cols-4 gap-3">
      <!-- Text search -->
      <input
        class="input text-sm"
        placeholder="Search by name, payload, or ID…"
        bind:value={searchText}
      />

      <!-- Category filter -->
      <select class="input text-sm" bind:value={selectedCategory}>
        {#each categories as cat}
          <option value={cat.value}>{cat.label}</option>
        {/each}
      </select>

      <!-- Time range -->
      <select class="input text-sm" bind:value={selectedTimeRange}>
        {#each timeRanges as range}
          <option value={range.value}>{range.label}</option>
        {/each}
      </select>

      <!-- Sort -->
      <select class="input text-sm" bind:value={sortBy}>
        <option value="newest">Newest First</option>
        <option value="oldest">Oldest First</option>
      </select>
    </div>
  </div>

  <!-- Category summary badges -->
  {#if Object.keys(stats.byCategory).length > 0}
    <div class="flex flex-wrap gap-2 mb-6">
      {#each Object.entries(stats.byCategory) as [cat, count]}
        <button
          class="px-3 py-1 rounded-full text-xs border transition-colors
            {selectedCategory === cat + '.' ? 'bg-nexora-accent text-white border-nexora-accent' : 'border-nexora-border text-nexora-muted hover:text-nexora-text'}"
          onclick={() => (selectedCategory = selectedCategory === cat + '.' ? '' : cat + '.')}
        >
          {cat} ({count})
        </button>
      {/each}
    </div>
  {/if}

  <!-- Audit table -->
  {#if filteredEvents.length === 0 && !loading}
    <div class="card text-center text-nexora-muted">
      {#if allEvents.length === 0}
        No audit events recorded yet.
      {:else}
        No events match the current filters.
      {/if}
    </div>
  {:else}
    <div class="card !p-0 overflow-hidden">
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border bg-nexora-bg/50">
              <th class="py-3 px-4">#</th>
              <th class="py-3 px-4">Category</th>
              <th class="py-3 px-4">Event Name</th>
              <th class="py-3 px-4">Payload</th>
              <th class="py-3 px-4">Timestamp</th>
              <th class="py-3 px-4">Age</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredEvents.slice(0, 200) as evt}
              <tr class="border-b border-nexora-border/50 hover:bg-nexora-bg/30">
                <td class="py-2 px-4 text-nexora-muted font-mono text-xs">{evt.id}</td>
                <td class="py-2 px-4">
                  <span class={categoryBadge(evt.name)}>{categoryOf(evt.name)}</span>
                </td>
                <td class="py-2 px-4">
                  <span class="font-mono text-xs {categoryColor(evt.name)}">{evt.name}</span>
                </td>
                <td class="py-2 px-4 font-mono text-xs text-nexora-text max-w-xs truncate" title={evt.payload}>
                  {evt.payload || '(empty)'}
                </td>
                <td class="py-2 px-4 text-xs text-nexora-muted whitespace-nowrap">{formatDate(evt.timestamp)}</td>
                <td class="py-2 px-4 text-xs text-nexora-muted whitespace-nowrap">{timeAgo(evt.timestamp)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if filteredEvents.length > 200}
        <div class="p-3 text-center text-xs text-nexora-muted border-t border-nexora-border">
          Showing 200 of {filteredEvents.length} events. Refine filters to see more.
        </div>
      {/if}
    </div>
  {/if}
</Layout>
