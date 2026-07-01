<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type NotificationItem } from '$lib/api/gateway';
  import { goto } from '$app/navigation';

  let unread = $state(0);
  let notifications = $state<NotificationItem[]>([]);
  let open = $state(false);
  let loading = $state(false);

  async function loadUnread() {
    try {
      const resp = await api.unreadCount();
      unread = resp.count;
    } catch { /* ignore */ }
  }

  async function loadAll() {
    loading = true;
    try {
      const resp = await api.listNotifications();
      notifications = resp.notifications;
      unread = resp.notifications.filter((n) => !n.read).length;
    } catch { /* ignore */ }
    loading = false;
  }

  async function handleMarkRead(id: string) {
    try {
      await api.markNotificationRead(id);
      notifications = notifications.map((n) =>
        n.id === id ? { ...n, read: true } : n,
      );
      unread = Math.max(0, unread - 1);
    } catch { /* ignore */ }
  }

  async function handleMarkAllRead() {
    try {
      await api.markAllNotificationsRead();
      notifications = notifications.map((n) => ({ ...n, read: true }));
      unread = 0;
    } catch { /* ignore */ }
  }

  function toggle() {
    open = !open;
    if (open) loadAll();
  }

  function handleClick(n: NotificationItem) {
    if (!n.read) handleMarkRead(n.id);
    if (n.link) {
      open = false;
      goto(n.link);
    }
  }

  function severityColor(s: string): string {
    switch (s) {
      case 'success': return 'text-emerald-400';
      case 'warning': return 'text-amber-400';
      case 'error': return 'text-red-400';
      default: return 'text-blue-400';
    }
  }

  function severityIcon(s: string): string {
    switch (s) {
      case 'success': return '✓';
      case 'warning': return '⚠';
      case 'error': return '✗';
      default: return 'ℹ';
    }
  }

  function timeAgo(ns: number): string {
    const ms = ns / 1_000_000;
    const diff = Date.now() - ms;
    const s = Math.floor(diff / 1000);
    if (s < 60) return 'just now';
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  // Poll unread count every 30 seconds.
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  // Listen for live notification events via SSE.
  let sseSub: { close: () => void } | null = null;

  onMount(() => {
    loadUnread();
    pollInterval = setInterval(loadUnread, 30_000);

    // Subscribe to notification.created events via SSE.
    sseSub = api.subscribeLiveEvents((evt) => {
      // If we receive a notification.created event, refresh unread count.
      if (evt.name === 'notification.created' || evt.name === 'notification') {
        loadUnread();
        if (open) loadAll();
      }
    });
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    sseSub?.close();
  });

  // Close on outside click.
  function handleWindowClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (open && !target.closest('[data-notification-bell]')) {
      open = false;
    }
  }

  $effect(() => {
    if (open) {
      window.addEventListener('click', handleWindowClick);
    }
    return () => window.removeEventListener('click', handleWindowClick);
  });
</script>

<div class="relative" data-notification-bell>
  <button
    class="relative p-2 rounded-md text-nexora-muted hover:text-nexora-text hover:bg-nexora-border/50 transition-colors"
    onclick={toggle}
    aria-label="Notifications"
  >
    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"
      />
    </svg>
    {#if unread > 0}
      <span
        class="absolute -top-0.5 -right-0.5 min-w-[18px] h-[18px] flex items-center justify-center px-1 text-[10px] font-bold rounded-full bg-red-500 text-white"
      >
        {unread > 99 ? '99+' : unread}
      </span>
    {/if}
  </button>

  {#if open}
    <div
      class="absolute right-0 top-full mt-2 w-80 max-h-[400px] bg-nexora-surface border border-nexora-border rounded-lg shadow-2xl overflow-hidden z-50"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-nexora-border">
        <span class="text-sm font-semibold">
          Notifications
          {#if unread > 0}
            <span class="text-xs text-nexora-muted ml-1">({unread} unread)</span>
          {/if}
        </span>
        {#if unread > 0}
          <button
            class="text-xs text-nexora-accent hover:underline"
            onclick={handleMarkAllRead}
          >
            Mark all read
          </button>
        {/if}
      </div>

      <!-- List -->
      <div class="max-h-[320px] overflow-y-auto">
        {#if loading}
          <div class="px-4 py-8 text-center text-sm text-nexora-muted">Loading…</div>
        {:else if notifications.length === 0}
          <div class="px-4 py-8 text-center text-sm text-nexora-muted">
            No notifications yet.
          </div>
        {:else}
          {#each notifications as n (n.id)}
            <button
              class="w-full flex gap-3 px-4 py-3 text-left border-b border-nexora-border/50 hover:bg-nexora-bg/50 transition-colors
                {n.read ? 'opacity-60' : ''}"
              onclick={() => handleClick(n)}
            >
              <!-- Severity icon -->
              <span class="shrink-0 w-6 h-6 rounded-full flex items-center justify-center text-xs
                bg-nexora-bg border border-nexora-border {severityColor(n.severity)}">
                {n.icon || severityIcon(n.severity)}
              </span>

              <!-- Content -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-sm font-medium truncate">{n.title}</span>
                  {#if !n.read}
                    <span class="shrink-0 w-2 h-2 rounded-full bg-nexora-accent"></span>
                  {/if}
                </div>
                <p class="text-xs text-nexora-muted mt-0.5 line-clamp-2">{n.body}</p>
                <span class="text-[10px] text-nexora-muted mt-1">{timeAgo(n.created_at)}</span>
              </div>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
