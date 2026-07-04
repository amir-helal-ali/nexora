<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface AuditEntry {
    id: string;
    actor: string;
    action: string;
    target: string;
    category: string;
    success: boolean;
    error: string | null;
    metadata: Record<string, string>;
    timestamp: number;
  }

  interface AuditStats {
    total: number;
    success: number;
    failure: number;
    by_category: { category: string; count: number }[];
  }

  let entries: AuditEntry[] = [];
  let stats: AuditStats | null = null;
  let loading = true;
  let error = '';
  let filterActor = '';
  let filterAction = '';
  let filterCategory = '';
  let filterSuccess: string = '';
  let total = 0;
  let limit = 50;

  async function loadEntries() {
    loading = true;
    error = '';
    try {
      const params = new URLSearchParams();
      params.set('limit', String(limit));
      if (filterActor) params.set('actor', filterActor);
      if (filterAction) params.set('action', filterAction);
      if (filterCategory) params.set('category', filterCategory);
      if (filterSuccess === 'true' || filterSuccess === 'false') {
        params.set('success', filterSuccess);
      }
      const resp = await api.request<{ entries: AuditEntry[]; total: number }>(
        `/api/audit/entries?${params}`
      );
      entries = resp.entries || [];
      total = resp.total;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل السجل';
    } finally {
      loading = false;
    }
  }

  async function loadStats() {
    try {
      stats = await api.request<AuditStats>('/api/audit/stats');
    } catch (e) {
      // تجاهل
    }
  }

  function formatTime(ns: number): string {
    if (!ns) return '';
    return new Date(ns / 1_000_000).toLocaleString('ar');
  }

  function categoryColor(cat: string): string {
    const colors: Record<string, string> = {
      auth: 'bg-purple-900 text-purple-300',
      billing: 'bg-green-900 text-green-300',
      rule: 'bg-blue-900 text-blue-300',
      sso: 'bg-yellow-900 text-yellow-300',
      notification: 'bg-pink-900 text-pink-300',
      system: 'bg-zinc-700 text-zinc-300',
    };
    return colors[cat] || 'bg-zinc-800 text-zinc-400';
  }

  function applyFilters() {
    loadEntries();
  }

  function clearFilters() {
    filterActor = '';
    filterAction = '';
    filterCategory = '';
    filterSuccess = '';
    loadEntries();
  }

  onMount(() => {
    loadEntries();
    loadStats();
  });
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">سجل التدقيق</h1>
      <p class="text-sm text-zinc-400 mt-1">{total} مدخل إجمالي</p>
    </div>
    <button
      onclick={() => { loadEntries(); loadStats(); }}
      class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
    >
      تحديث
    </button>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">
      {error}
    </div>
  {/if}

  <!-- إحصائيات -->
  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{stats.total}</div>
        <div class="text-xs text-zinc-500 mt-1">الإجمالي</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-green-400">{stats.success}</div>
        <div class="text-xs text-zinc-500 mt-1">ناجح</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-red-400">{stats.failure}</div>
        <div class="text-xs text-zinc-500 mt-1">فاشل</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-blue-400">{stats.by_category?.length || 0}</div>
        <div class="text-xs text-zinc-500 mt-1">فئات</div>
      </div>
    </div>
  {/if}

  <!-- فلاتر -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
    <div class="grid grid-cols-1 md:grid-cols-5 gap-3">
      <input
        bind:value={filterActor}
        placeholder="الفاعل"
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
      <input
        bind:value={filterAction}
        placeholder="الإجراء"
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
      <input
        bind:value={filterCategory}
        placeholder="الفئة"
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
      <select
        bind:value={filterSuccess}
        class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      >
        <option value="">الكل</option>
        <option value="true">ناجح فقط</option>
        <option value="false">فاشل فقط</option>
      </select>
      <div class="flex gap-2">
        <button
          onclick={applyFilters}
          class="flex-1 px-3 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-sm transition"
        >
          تطبيق
        </button>
        <button
          onclick={clearFilters}
          class="px-3 py-2 bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded-lg text-sm transition"
        >
          مسح
        </button>
      </div>
    </div>
  </div>

  <!-- قائمة المدخلات -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if entries.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد مدخلات</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each entries as e (e.id)}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class={`px-2 py-0.5 text-xs rounded-full font-mono ${categoryColor(e.category)}`}>
                    {e.category}
                  </span>
                  <span class="text-sm font-medium text-white">{e.action}</span>
                  {#if e.success}
                    <span class="text-green-400 text-xs">✓</span>
                  {:else}
                    <span class="text-red-400 text-xs">✕</span>
                  {/if}
                </div>
                <div class="mt-1 text-xs text-zinc-500 flex gap-4">
                  <span>الفاعل: <code class="text-zinc-400">{e.actor}</code></span>
                  <span>الهدف: <code class="text-zinc-400">{e.target}</code></span>
                </div>
                {#if e.error}
                  <div class="mt-1 text-xs text-red-400">{e.error}</div>
                {/if}
              </div>
              <div class="text-xs text-zinc-600 whitespace-nowrap">
                {formatTime(e.timestamp)}
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
