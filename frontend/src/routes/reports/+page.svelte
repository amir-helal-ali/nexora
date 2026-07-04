<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface SecurityReport {
    period: string;
    from: number;
    to: number;
    total_alerts: number;
    alerts_by_severity: Record<string, number>;
    alerts_by_type: Record<string, number>;
    alerts_by_status: Record<string, number>;
    top_alerted_actors: [string, number][];
    total_audit_entries: number;
    successful_entries: number;
    failed_entries: number;
    entries_by_category: Record<string, number>;
    top_actions: [string, number][];
    top_actors: [string, number][];
    summary: string;
  }

  let report: SecurityReport | null = null;
  let loading = true;
  let error = '';
  let currentPeriod = 'daily';

  async function loadReport(period: string) {
    loading = true;
    error = '';
    currentPeriod = period;
    try {
      const resp = await api.request<{ report: SecurityReport }>(`/api/security/reports/${period}`);
      report = resp.report;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل التقرير';
    } finally {
      loading = false;
    }
  }

  function severityColor(s: string): string {
    return {
      critical: 'bg-red-900 text-red-300',
      high: 'bg-orange-900 text-orange-300',
      medium: 'bg-yellow-900 text-yellow-300',
      low: 'bg-blue-900 text-blue-300',
      info: 'bg-zinc-700 text-zinc-300',
    }[s] || 'bg-zinc-800 text-zinc-400';
  }

  function formatTime(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleString('ar');
  }

  function failureRate(): string {
    if (!report || report.total_audit_entries === 0) return '0.0%';
    return ((report.failed_entries / report.total_audit_entries) * 100).toFixed(1) + '%';
  }

  onMount(() => loadReport('daily'));
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">التقارير الأمنية</h1>
      <p class="text-sm text-zinc-400 mt-1">ملخص النشاط الأمني</p>
    </div>
  </div>

  <!-- اختيار الفترة -->
  <div class="flex gap-2">
    {#each ['daily', 'weekly', 'monthly'] as p}
      <button
        onclick={() => loadReport(p)}
        class={`px-4 py-2 rounded-lg text-sm transition ${
          currentPeriod === p
            ? 'bg-blue-600 text-white'
            : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-300'
        }`}
      >
        {p === 'daily' ? 'يومي' : p === 'weekly' ? 'أسبوعي' : 'شهري'}
      </button>
    {/each}
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if loading}
    <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
  {:else if report}
    <!-- الملخص -->
    <div class="bg-blue-900/30 border border-blue-700 rounded-xl p-4">
      <p class="text-sm text-blue-200">{report.summary}</p>
      <p class="text-xs text-zinc-500 mt-2">
        الفترة: {formatTime(report.from)} ← {formatTime(report.to)}
      </p>
    </div>

    <!-- الإحصائيات الرئيسية -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-red-400">{report.total_alerts}</div>
        <div class="text-xs text-zinc-500 mt-1">تنبيهات أمنية</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{report.total_audit_entries}</div>
        <div class="text-xs text-zinc-500 mt-1">مدخلات تدقيق</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-green-400">{report.successful_entries}</div>
        <div class="text-xs text-zinc-500 mt-1">ناجحة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-red-400">{report.failed_entries}</div>
        <div class="text-xs text-zinc-500 mt-1">فاشلة ({failureRate()})</div>
      </div>
    </div>

    <!-- التنبيهات حسب الخطورة -->
    {#if Object.keys(report.alerts_by_severity).length > 0}
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">التنبيهات حسب الخطورة</h3>
        <div class="space-y-2">
          {#each Object.entries(report.alerts_by_severity) as [sev, count]}
            <div class="flex items-center justify-between">
              <span class={`px-2 py-0.5 text-xs rounded-full font-mono ${severityColor(sev)}`}>{sev}</span>
              <span class="text-sm text-zinc-400">{count}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- التنبيهات حسب النوع -->
    {#if Object.keys(report.alerts_by_type).length > 0}
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">التنبيهات حسب النوع</h3>
        <div class="space-y-2">
          {#each Object.entries(report.alerts_by_type) as [type, count]}
            <div class="flex items-center justify-between">
              <code class="text-xs text-zinc-400">{type}</code>
              <span class="text-sm text-zinc-300">{count}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- المدخلات حسب الفئة -->
    {#if Object.keys(report.entries_by_category).length > 0}
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">المدخلات حسب الفئة</h3>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-2">
          {#each Object.entries(report.entries_by_category) as [cat, count]}
            <div class="flex items-center justify-between bg-zinc-800/50 px-3 py-2 rounded-lg">
              <span class="text-xs text-zinc-400">{cat}</span>
              <span class="text-sm text-zinc-200 font-mono">{count}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- أكثر الفاعلين نشاطاً -->
    {#if report.top_actors.length > 0}
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">أكثر الفاعلين نشاطاً</h3>
        <div class="space-y-2">
          {#each report.top_actors.slice(0, 5) as [actor, count]}
            <div class="flex items-center justify-between">
              <code class="text-xs text-zinc-400">{actor}</code>
              <span class="text-sm text-zinc-300">{count} إجراء</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- أكثر الإجراءات -->
    {#if report.top_actions.length > 0}
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">أكثر الإجراءات</h3>
        <div class="space-y-2">
          {#each report.top_actions.slice(0, 5) as [action, count]}
            <div class="flex items-center justify-between">
              <code class="text-xs text-zinc-400">{action}</code>
              <span class="text-sm text-zinc-300">{count}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>
