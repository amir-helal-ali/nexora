<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface InAppNotification {
    id: string;
    user_id: string;
    title: string;
    body: string;
    action_url: string | null;
    created_at: number;
    read_at: number | null;
  }

  let notifications: InAppNotification[] = [];
  let unreadCount = 0;
  let loading = true;
  let error = '';
  let newTitle = '';
  let newBody = '';
  let newUserId = '';
  let sending = false;

  async function loadNotifications() {
    loading = true;
    error = '';
    try {
      const resp = await api.request<{ notifications: InAppNotification[]; count: number }>(
        '/api/notifications?limit=50'
      );
      notifications = resp.notifications || [];
      const unreadResp = await api.request<{ unread: number }>(
        '/api/notifications/unread-count'
      );
      unreadCount = unreadResp.unread;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل الإشعارات';
    } finally {
      loading = false;
    }
  }

  async function markRead(id: string) {
    try {
      await api.request(`/api/notifications/${id}/read`, { method: 'POST' });
      await loadNotifications();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التحديث';
    }
  }

  async function markAllRead() {
    try {
      await api.request('/api/notifications/read-all', { method: 'POST' });
      await loadNotifications();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التحديث';
    }
  }

  async function deleteNotification(id: string) {
    try {
      await api.request(`/api/notifications/${id}`, { method: 'DELETE' });
      await loadNotifications();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الحذف';
    }
  }

  async function sendNotification() {
    if (!newTitle || !newBody || !newUserId) {
      error = 'يرجى ملء جميع الحقول';
      return;
    }
    sending = true;
    error = '';
    try {
      await api.request('/api/notifications', {
        method: 'POST',
        body: JSON.stringify({
          user_id: newUserId,
          title: newTitle,
          body: newBody
        })
      });
      newTitle = '';
      newBody = '';
      newUserId = '';
      await loadNotifications();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الإرسال';
    } finally {
      sending = false;
    }
  }

  function formatTime(ns: number): string {
    if (!ns) return '';
    const d = new Date(ns / 1_000_000);
    return d.toLocaleString('ar');
  }

  onMount(loadNotifications);
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">الإشعارات</h1>
      <p class="text-sm text-zinc-400 mt-1">
        {notifications.length} إشعار · {unreadCount} غير مقروء
      </p>
    </div>
    <div class="flex gap-2">
      <button
        onclick={loadNotifications}
        class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
      >
        تحديث
      </button>
      <button
        onclick={markAllRead}
        disabled={unreadCount === 0}
        class="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded-lg text-sm transition"
      >
        تعليم الكل كمقروء
      </button>
    </div>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">
      {error}
    </div>
  {/if}

  <!-- إرسال إشعار جديد -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
    <h2 class="text-lg font-semibold text-white mb-4">إرسال إشعار جديد</h2>
    <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
      <input
        bind:value={newUserId}
        placeholder="معرّف المستخدم"
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
      <input
        bind:value={newTitle}
        placeholder="العنوان"
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
      <input
        bind:value={newBody}
        placeholder="المحتوى"
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
    </div>
    <button
      onclick={sendNotification}
      disabled={sending}
      class="mt-3 px-4 py-2 bg-green-600 hover:bg-green-500 disabled:opacity-50 text-white rounded-lg text-sm transition"
    >
      {sending ? 'جارٍ الإرسال...' : 'إرسال'}
    </button>
  </div>

  <!-- قائمة الإشعارات -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if notifications.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد إشعارات</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each notifications as n (n.id)}
          <div
            class="p-4 hover:bg-zinc-800/50 transition flex items-start gap-3"
            class:bg-blue-950/30={!n.read_at}
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <h3 class="text-sm font-medium text-white truncate">{n.title}</h3>
                {!n.read_at && (
                  <span class="inline-block w-2 h-2 bg-blue-500 rounded-full flex-shrink-0"></span>
                )}
              </div>
              <p class="text-sm text-zinc-400 mt-1">{n.body}</p>
              <p class="text-xs text-zinc-600 mt-1">{formatTime(n.created_at)}</p>
            </div>
            <div class="flex gap-1 flex-shrink-0">
              {!n.read_at && (
                <button
                  onclick={() => markRead(n.id)}
                  class="px-2 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded transition"
                  title="تعليم كمقروء"
                >
                  ✓
                </button>
              )}
              <button
                onclick={() => deleteNotification(n.id)}
                class="px-2 py-1 text-xs bg-red-900/50 hover:bg-red-800 text-red-200 rounded transition"
                title="حذف"
              >
                ✕
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
