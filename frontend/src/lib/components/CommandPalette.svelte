<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type PackageInfo } from '$lib/api/gateway';

  let { open = $bindable(false) }: { open: boolean } = $props();

  let query = $state('');
  let selectedIndex = $state(0);
  let inputEl: HTMLInputElement | null = $state(null);
  let packageResults = $state<PackageInfo[]>([]);
  let searchLoading = $state(false);

  interface Command {
    id: string;
    label: string;
    hint?: string;
    icon: string;
    section: string;
    action: () => void;
  }

  const navCommands: Command[] = [
    { id: 'nav-dashboard', label: 'Dashboard', hint: '/', icon: '▤', section: 'Navigate', action: () => goto('/') },
    { id: 'nav-events', label: 'Events', hint: '/events', icon: '↻', section: 'Navigate', action: () => goto('/events') },
    { id: 'nav-modules', label: 'Modules', hint: '/modules', icon: '▣', section: 'Navigate', action: () => goto('/modules') },
    { id: 'nav-marketplace', label: 'Marketplace', hint: '/marketplace', icon: '◇', section: 'Navigate', action: () => goto('/marketplace') },
    { id: 'nav-billing', label: 'Billing', hint: '/billing', icon: '$', section: 'Navigate', action: () => goto('/billing') },
    { id: 'nav-health', label: 'System Health', hint: '/health', icon: '♥', section: 'Navigate', action: () => goto('/health') },
    { id: 'nav-settings', label: 'Settings', hint: '/settings', icon: '⚙', section: 'Navigate', action: () => goto('/settings') },
    { id: 'nav-cluster', label: 'Cluster', hint: '/cluster', icon: '🕸', section: 'Navigate', action: () => goto('/cluster') },
    { id: 'nav-workflows', label: 'Workflows', hint: '/workflows', icon: '⚙', section: 'Navigate', action: () => goto('/workflows') },
  ];

  const actionCommands: Command[] = [
    {
      id: 'action-ping',
      label: 'Send PING to Core',
      icon: '⚡',
      section: 'Actions',
      action: async () => {
        try {
          const resp = await api.ping();
          alert(`PONG: ${JSON.stringify(resp)}`);
        } catch (e) {
          alert(`Ping failed: ${e instanceof Error ? e.message : 'unknown'}`);
        }
      },
    },
    {
      id: 'action-check-updates',
      label: 'Check for Package Updates',
      icon: '🔄',
      section: 'Actions',
      action: async () => {
        try {
          const resp = await api.request<{ ok: boolean; updates_available: number }>('/api/marketplace/updates/check');
          alert(`Updates available: ${resp.updates_available}`);
        } catch (e) {
          alert(`Check failed: ${e instanceof Error ? e.message : 'unknown'}`);
        }
      },
    },
    {
      id: 'action-process-updates',
      label: 'Process Auto-Updates',
      icon: '⬆',
      section: 'Actions',
      action: async () => {
        try {
          const resp = await api.request<{ ok: boolean; processed: number; succeeded: number }>('/api/marketplace/updates/process', { method: 'POST' });
          alert(`Processed: ${resp.processed}, Succeeded: ${resp.succeeded}`);
        } catch (e) {
          alert(`Update failed: ${e instanceof Error ? e.message : 'unknown'}`);
        }
      },
    },
    {
      id: 'action-billing-stats',
      label: 'View Billing Stats',
      icon: '$',
      section: 'Actions',
      action: () => goto('/billing'),
    },
    { id: 'action-logout', label: 'Sign out', icon: '⏏', section: 'Actions', action: () => goto('/logout') },
    { id: 'action-openapi', label: 'View API Spec (OpenAPI)', icon: '📄', section: 'Actions', action: () => window.open('/api/openapi.json', '_blank') },
  ];

  let allCommands = $derived([...navCommands, ...actionCommands]);

  let filteredCommands = $derived(
    query.trim() === ''
      ? allCommands
      : allCommands.filter((c) => c.label.toLowerCase().includes(query.toLowerCase())),
  );

  let packageCommands = $derived(
    packageResults.slice(0, 5).map((pkg) => ({
      id: `pkg-${pkg.manifest.id}`,
      label: pkg.manifest.name,
      hint: pkg.manifest.id,
      icon: '📦',
      section: 'Packages',
      action: () => goto('/marketplace'),
    })),
  );

  let visibleCommands = $derived([...filteredCommands, ...packageCommands]);

  $effect(() => { query; selectedIndex = 0; });

  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const q = query.trim();
    if (searchTimer) clearTimeout(searchTimer);
    if (q.length < 2) { packageResults = []; searchLoading = false; return; }
    searchLoading = true;
    searchTimer = setTimeout(async () => {
      try {
        const resp = await api.searchPackages(q);
        packageResults = resp.packages;
      } catch { packageResults = []; } finally { searchLoading = false; }
    }, 200);
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') { e.preventDefault(); selectedIndex = Math.min(selectedIndex + 1, visibleCommands.length - 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); selectedIndex = Math.max(selectedIndex - 1, 0); }
    else if (e.key === 'Enter') { e.preventDefault(); const cmd = visibleCommands[selectedIndex]; if (cmd) { cmd.action(); open = false; } }
    else if (e.key === 'Escape') { e.preventDefault(); open = false; }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); open = !open; }
  }

  onMount(() => { window.addEventListener('keydown', handleGlobalKeydown); });
  onDestroy(() => { window.removeEventListener('keydown', handleGlobalKeydown); });

  $effect(() => {
    if (open) { query = ''; selectedIndex = 0; setTimeout(() => inputEl?.focus(), 0); }
  });

  let grouped = $derived.by(() => {
    const groups: Record<string, Command[]> = {};
    for (const cmd of visibleCommands) { if (!groups[cmd.section]) groups[cmd.section] = []; groups[cmd.section].push(cmd); }
    return groups;
  });
</script>

{#if open}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm" onclick={() => (open = false)} role="button" tabindex="0" aria-label="Close command palette"></div>
  <div class="fixed top-[15%] left-1/2 -translate-x-1/2 z-50 w-full max-w-xl" role="dialog" aria-label="Command palette">
    <div class="card !p-0 overflow-hidden shadow-2xl">
      <div class="flex items-center gap-3 px-4 py-3 border-b border-nexora-border">
        <svg class="w-4 h-4 text-nexora-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" /></svg>
        <input bind:this={inputEl} bind:value={query} onkeydown={handleKeydown} class="flex-1 bg-transparent text-nexora-text placeholder:text-nexora-muted focus:outline-none text-sm" placeholder="Search commands, pages, packages…" aria-label="Search" />
        {#if searchLoading}<svg class="w-4 h-4 text-nexora-muted animate-spin" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" /><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>{/if}
        <kbd class="text-xs text-nexora-muted px-1.5 py-0.5 rounded border border-nexora-border">ESC</kbd>
      </div>
      <div class="max-h-[60vh] overflow-y-auto py-2">
        {#if visibleCommands.length === 0}
          <div class="px-4 py-8 text-center text-sm text-nexora-muted">No results for "{query}"</div>
        {:else}
          {#each Object.entries(grouped) as [section, cmds]}
            <div class="px-2">
              <div class="px-2 py-1 text-xs font-semibold uppercase tracking-wider text-nexora-muted">{section}</div>
              {#each cmds as cmd}
                {@const flatIndex = visibleCommands.indexOf(cmd)}
                <button class="w-full flex items-center gap-3 px-2 py-2 rounded-md text-sm transition-colors {flatIndex === selectedIndex ? 'bg-nexora-accent text-white' : 'text-nexora-text hover:bg-nexora-border'}" onclick={() => { cmd.action(); open = false; }} onmouseenter={() => (selectedIndex = flatIndex)}>
                  <span class="w-5 text-center {flatIndex === selectedIndex ? 'text-white' : 'text-nexora-muted'}">{cmd.icon}</span>
                  <span class="flex-1 text-left">{cmd.label}</span>
                  {#if cmd.hint}<span class="text-xs font-mono {flatIndex === selectedIndex ? 'text-white/70' : 'text-nexora-muted'}">{cmd.hint}</span>{/if}
                </button>
              {/each}
            </div>
          {/each}
        {/if}
      </div>
      <div class="flex items-center justify-between px-4 py-2 border-t border-nexora-border text-xs text-nexora-muted">
        <div class="flex items-center gap-3">
          <span class="flex items-center gap-1"><kbd class="px-1.5 py-0.5 rounded border border-nexora-border">↑</kbd><kbd class="px-1.5 py-0.5 rounded border border-nexora-border">↓</kbd>navigate</span>
          <span class="flex items-center gap-1"><kbd class="px-1.5 py-0.5 rounded border border-nexora-border">↵</kbd>select</span>
        </div>
        <span>Nexora Command Palette</span>
      </div>
    </div>
  </div>
{/if}
