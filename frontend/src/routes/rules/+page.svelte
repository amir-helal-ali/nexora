<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface Rule {
    id: string;
    name: string;
    description: string;
    status: 'enabled' | 'disabled' | 'deleted';
    priority: number;
    execution_count: number;
    success_count: number;
    failure_count: number;
    last_executed_at: number | null;
  }

  interface RuleStats {
    total_rules: number;
    enabled_rules: number;
    disabled_rules: number;
    total_executions: number;
    total_successes: number;
    total_failures: number;
  }

  let rules: Rule[] = [];
  let stats: RuleStats | null = null;
  let loading = true;
  let error = '';
  let showCreate = false;
  let newName = '';
  let newCondition = '{"type":"always"}';
  let newActions = '[]';

  async function loadRules() {
    loading = true;
    error = '';
    try {
      const resp = await api.request<{ rules: Rule[]; count: number }>('/api/rules');
      rules = resp.rules || [];
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل القواعد';
    } finally {
      loading = false;
    }
  }

  async function loadStats() {
    try {
      stats = await api.request<RuleStats>('/api/rules/stats');
    } catch (e) {
      // تجاهل
    }
  }

  async function createRule() {
    error = '';
    if (!newName) {
      error = 'اسم القاعدة مطلوب';
      return;
    }
    let condition, actions;
    try {
      condition = JSON.parse(newCondition);
      actions = JSON.parse(newActions);
    } catch (e) {
      error = 'JSON غير صالح في الشرط أو الإجراءات';
      return;
    }
    try {
      await api.request('/api/rules', {
        method: 'POST',
        body: JSON.stringify({ name: newName, condition, actions })
      });
      showCreate = false;
      newName = '';
      newCondition = '{"type":"always"}';
      newActions = '[]';
      await loadRules();
      await loadStats();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الإنشاء';
    }
  }

  async function deleteRule(id: string) {
    if (!confirm('هل أنت متأكد من حذف هذه القاعدة؟')) return;
    try {
      await api.request(`/api/rules/${id}`, { method: 'DELETE' });
      await loadRules();
      await loadStats();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الحذف';
    }
  }

  async function toggleRule(id: string, currentStatus: string) {
    const action = currentStatus === 'enabled' ? 'disable' : 'enable';
    try {
      await api.request(`/api/rules/${id}/${action}`, { method: 'POST' });
      await loadRules();
      await loadStats();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التبديل';
    }
  }

  function formatTime(ns: number | null): string {
    if (!ns) return 'أبداً';
    return new Date(ns / 1_000_000).toLocaleString('ar');
  }

  function successRate(rule: Rule): string {
    if (rule.execution_count === 0) return '—';
    return ((rule.success_count / rule.execution_count) * 100).toFixed(1) + '%';
  }

  onMount(() => {
    loadRules();
    loadStats();
  });
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">محرك القواعد</h1>
      <p class="text-sm text-zinc-400 mt-1">أتمتة يحركها الأحداث</p>
    </div>
    <div class="flex gap-2">
      <button
        onclick={() => { loadRules(); loadStats(); }}
        class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
      >
        تحديث
      </button>
      <button
        onclick={() => (showCreate = !showCreate)}
        class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-sm transition"
      >
        {showCreate ? 'إلغاء' : '+ قاعدة جديدة'}
      </button>
    </div>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">
      {error}
    </div>
  {/if}

  <!-- إحصائيات -->
  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-6 gap-3">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-3">
        <div class="text-xl font-bold text-white">{stats.total_rules}</div>
        <div class="text-xs text-zinc-500">الإجمالي</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-3">
        <div class="text-xl font-bold text-green-400">{stats.enabled_rules}</div>
        <div class="text-xs text-zinc-500">مفعّلة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-3">
        <div class="text-xl font-bold text-zinc-500">{stats.disabled_rules}</div>
        <div class="text-xs text-zinc-500">معطّلة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-3">
        <div class="text-xl font-bold text-blue-400">{stats.total_executions}</div>
        <div class="text-xs text-zinc-500">تنفيذاً</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-3">
        <div class="text-xl font-bold text-green-400">{stats.total_successes}</div>
        <div class="text-xs text-zinc-500">نجاح</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-3">
        <div class="text-xl font-bold text-red-400">{stats.total_failures}</div>
        <div class="text-xs text-zinc-500">فشل</div>
      </div>
    </div>
  {/if}

  <!-- نموذج الإنشاء -->
  {#if showCreate}
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 space-y-3">
      <h2 class="text-sm font-semibold text-white">إنشاء قاعدة جديدة</h2>
      <input
        bind:value={newName}
        placeholder="اسم القاعدة"
        class="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
      />
      <div>
        <label class="text-xs text-zinc-500">الشرط (JSON):</label>
        <textarea
          bind:value={newCondition}
          rows="3"
          class="w-full mt-1 bg-zinc-800 text-green-400 px-3 py-2 rounded-lg border border-zinc-700 text-sm font-mono"
        ></textarea>
      </div>
      <div>
        <label class="text-xs text-zinc-500">الإجراءات (JSON array):</label>
        <textarea
          bind:value={newActions}
          rows="3"
          class="w-full mt-1 bg-zinc-800 text-green-400 px-3 py-2 rounded-lg border border-zinc-700 text-sm font-mono"
        ></textarea>
      </div>
      <button
        onclick={createRule}
        class="px-4 py-2 bg-green-600 hover:bg-green-500 text-white rounded-lg text-sm transition"
      >
        إنشاء
      </button>
    </div>
  {/if}

  <!-- قائمة القواعد -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if rules.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد قواعد. أنشئ أول قاعدة!</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each rules as r (r.id)}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <h3 class="text-sm font-medium text-white">{r.name}</h3>
                  <span
                    class={`px-2 py-0.5 text-xs rounded-full ${
                      r.status === 'enabled'
                        ? 'bg-green-900 text-green-300'
                        : 'bg-zinc-700 text-zinc-400'
                    }`}
                  >
                    {r.status === 'enabled' ? 'مفعّلة' : 'معطّلة'}
                  </span>
                </div>
                <div class="mt-1 flex gap-4 text-xs text-zinc-500">
                  <span>تنفيذ: {r.execution_count}</span>
                  <span>نجاح: {r.success_count}</span>
                  <span>فشل: {r.failure_count}</span>
                  <span>معدل النجاح: {successRate(r)}</span>
                  <span>آخر تنفيذ: {formatTime(r.last_executed_at)}</span>
                </div>
              </div>
              <div class="flex gap-1 flex-shrink-0">
                <button
                  onclick={() => toggleRule(r.id, r.status)}
                  class="px-2 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded transition"
                >
                  {r.status === 'enabled' ? 'تعطيل' : 'تفعيل'}
                </button>
                <button
                  onclick={() => deleteRule(r.id)}
                  class="px-2 py-1 text-xs bg-red-900/50 hover:bg-red-800 text-red-200 rounded transition"
                >
                  حذف
                </button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
