<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';
  import { goto } from '$app/navigation';
  import Layout from '$lib/components/Layout.svelte';

  interface SearchResult {
    type: string;
    id: string | number;
    title: string;
    description: string;
    link: string | null;
    timestamp?: number;
  }

  let query = $state('');
  let results = $state<SearchResult[]>([]);
  let loading = $state(false);
  let hasSearched = $state(false);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  async function search() {
    if (query.trim().length < 2) {
      results = [];
      hasSearched = false;
      return;
    }
    loading = true;
    hasSearched = true;
    try {
      const resp = await api.request<{ ok: boolean; results: SearchResult[] }>(`/api/search?q=${encodeURIComponent(query)}`);
      results = resp.results || [];
    } catch {
      results = [];
    } finally {
      loading = false;
    }
  }

  function handleInput() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(search, 300);
  }

  function typeIcon(type: string): string {
    const map: Record<string, string> = {
      event: '↻', package: '📦', user: '👤', workflow: '⚙', notification: '🔔',
    };
    return map[type] || '⚪';
  }

  function typeColor(type: string): string {
    const map: Record<string, string> = {
      event: 'text-blue-400', package: 'text-purple-400', user: 'text-emerald-400',
      workflow: 'text-orange-400', notification: 'text-indigo-400',
    };
    return map[type] || 'text-nexora-muted';
  }

  function handleClick(r: SearchResult) {
    if (r.link) goto(r.link);
  }

  // Group results by type.
  let grouped = $derived.by(() => {
    const groups: Record<string, SearchResult[]> = {};
    for (const r of results) {
      if (!groups[r.type]) groups[r.type] = [];
      groups[r.type].push(r);
    }
    return groups;
  });

  onMount(() => {});
</script>

<Layout>
  <div class="mb-8">
    <h1 class="text-2xl font-semibold mb-1">Search</h1>
    <p class="text-sm text-nexora-muted">Search across events, packages, users, workflows, and notifications</p>
  </div>

  <!-- Search bar -->
  <div class="card mb-6">
    <div class="flex items-center gap-3">
      <svg class="w-5 h-5 text-nexora-muted shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
      <input
        class="flex-1 bg-transparent text-lg text-nexora-text placeholder:text-nexora-muted focus:outline-none"
        placeholder="Search everything…"
        bind:value={query}
        oninput={handleInput}
        autofocus
      />
      {#if loading}
        <svg class="w-5 h-5 text-nexora-muted animate-spin shrink-0" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
      {/if}
    </div>
  </div>

  <!-- Results -->
  {#if hasSearched && !loading}
    {#if results.length === 0}
      <div class="card text-center text-nexora-muted">
        No results for "{query}". Try a different search term.
      </div>
    {:else}
      <p class="text-sm text-nexora-muted mb-4">{results.length} result{results.length === 1 ? '' : 's'} for "{query}"</p>
      {#each Object.entries(grouped) as [type, items]}
        <div class="mb-6">
          <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-2 flex items-center gap-2">
            <span class={typeColor(type)}>{typeIcon(type)}</span>
            {type} ({items.length})
          </h3>
          <div class="space-y-2">
            {#each items as r}
              <button
                class="w-full flex items-center gap-3 p-3 rounded-lg bg-nexora-surface border border-nexora-border hover:border-nexora-accent/50 transition-colors text-left"
                onclick={() => handleClick(r)}
              >
                <span class="shrink-0 w-8 h-8 rounded-full flex items-center justify-center bg-nexora-bg border border-nexora-border {typeColor(r.type)}">
                  {typeIcon(r.type)}
                </span>
                <div class="flex-1 min-w-0">
                  <p class="text-sm font-medium truncate">{r.title}</p>
                  <p class="text-xs text-nexora-muted truncate">{r.description || '(no description)'}</p>
                </div>
                {#if r.link}
                  <svg class="w-4 h-4 text-nexora-muted shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/each}
    {/if}
  {:else if !hasSearched}
    <div class="card text-center text-nexora-muted">
      Start typing to search across the entire platform.
    </div>
  {/if}
</Layout>
