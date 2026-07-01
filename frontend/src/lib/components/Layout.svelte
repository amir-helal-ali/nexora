<script lang="ts">
  import { page } from '$app/stores';
  import CommandPalette from './CommandPalette.svelte';
  import NotificationBell from './NotificationBell.svelte';

  let { children } = $props();
  let paletteOpen = $state(false);

  const navItems = [
    { href: '/', label: 'Dashboard' },
    { href: '/metrics', label: 'Metrics' },
    { href: '/events', label: 'Events' },
    { href: '/audit', label: 'Audit' },
    { href: '/modules', label: 'Modules' },
    { href: '/marketplace', label: 'Marketplace' },
    { href: '/billing', label: 'Billing' },
    { href: '/workflows', label: 'Workflows' },
    { href: '/cluster', label: 'Cluster' },
    { href: '/terminal', label: 'Terminal' },
    { href: '/api-explorer', label: 'API' },
    { href: '/health', label: 'Health' },
    { href: '/organizations', label: 'Orgs' },
    { href: '/settings', label: 'Settings' },
  ];
</script>

<CommandPalette bind:open={paletteOpen} />

<div class="min-h-screen flex flex-col">
  <!-- Top bar -->
  <header class="border-b border-nexora-border bg-nexora-surface/50 backdrop-blur sticky top-0 z-40">
    <div class="mx-auto max-w-7xl px-6 h-14 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <a href="/" class="flex items-center gap-2">
          <div class="w-7 h-7 rounded bg-nexora-accent flex items-center justify-center">
            <span class="text-white font-bold text-sm">N</span>
          </div>
          <span class="font-semibold">Nexora</span>
          <span class="text-xs text-nexora-muted ml-1">v0.1.0</span>
        </a>
      </div>
      <nav class="hidden md:flex items-center gap-1">
        {#each navItems as item}
          <a
            href={item.href}
            class="px-3 py-1.5 rounded-md text-sm transition-colors
              {$page.url.pathname === item.href
                ? 'bg-nexora-border text-nexora-text'
                : 'text-nexora-muted hover:text-nexora-text hover:bg-nexora-border/50'}"
          >
            {item.label}
          </a>
        {/each}
      </nav>
      <div class="flex items-center gap-3">
        <!-- Command palette trigger -->
        <button
          onclick={() => (paletteOpen = true)}
          class="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs text-nexora-muted border border-nexora-border hover:bg-nexora-border hover:text-nexora-text transition-colors"
          title="Open command palette"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <span class="hidden sm:inline">Search…</span>
          <kbd class="hidden sm:inline px-1 py-0.5 rounded border border-nexora-border text-[10px]">⌘K</kbd>
        </button>
        <!-- Notification bell -->
        <NotificationBell />
        <a href="/logout" class="btn-ghost text-xs px-2 py-1">Logout</a>
      </div>
    </div>
  </header>

  <!-- Page content -->
  <main class="flex-1 mx-auto max-w-7xl w-full px-6 py-8">
    {@render children()}
  </main>

  <!-- Footer -->
  <footer class="border-t border-nexora-border py-4">
    <div class="mx-auto max-w-7xl px-6 flex items-center justify-between text-xs text-nexora-muted">
      <span>Nexora Cloud Operating System</span>
      <span>HTTP → NXP → Core → Auth · Press ⌘K</span>
    </div>
  </footer>
</div>
