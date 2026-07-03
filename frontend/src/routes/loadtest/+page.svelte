<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  let reports: any[] = [];
  let loading = true;
  let error = '';
  let generating = false;
  let selectedPeriod = 'daily';
  let lastReport: any = null;

  async function loadReports() {
    loading = true;
    error = '';
    try {
      const resp = await api.request<{ reports: any[]; count: number }>('/api/monitoring/reports');
      reports = resp.reports || [];
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التحميل';
    } finally {
      loading = false;
    }
  }

  async function generateReport() {
    generating = true;
    error = '';
    try {
      const resp = await api.request<{ report: any; ok: boolean }>(
        '/api/monitoring/reports/generate',
        {
          method: 'POST',
          body: JSON.stringify({ period: selectedPeriod })
        }
      );
      lastReport = resp.report;
      await loadReports();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التوليد';
    } finally {
      generating = false;
    }
  }

  function formatTime(ns: number): string {
    if (!ns) return '—';
    return new Date(ns / 1_000_000).toLocaleString('ar');
  }

  function periodLabel(p: string): string {
    return { hourly: 'ساعي', daily: 'يومي', weekly: 'أسبوعي' }[p] || p;
  }

  onMount(loadReports);
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">التقارير المجدولة</h1>
      <p class="text-sm text-zinc-400 mt-1">{reports.length} تقرير مُولّد</p>
    </div>
    <button onclick={loadReports} class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition">تحديث</button>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  <!-- توليد تقرير فوري -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
    <h2 class="text-sm font-semibold text-white mb-3">توليد تقرير فوري</h2>
    <div class="flex gap-3 items-center">
      <select bind:value={selectedPeriod} class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm">
        <option value="hourly">ساعي</option>
        <option value="daily">يومي</option>
        <option value="weekly">أسبوعي</option>
      </select>
      <button
        onclick={generateReport}
        disabled={generating}
        class="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded-lg text-sm transition"
      >
        {generating ? 'جارٍ التوليد...' : 'توليد'}
      </button>
    </div>
    {#if lastReport}
      <div class="mt-4 bg-zinc-800/50 p-4 rounded-lg">
        <div class="text-sm text-blue-300 mb-2">{lastReport.summary}</div>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
          <div><span class="text-zinc-500">الطلبات:</span> <span class="text-white">{lastReport.total_requests}</span></div>
          <div><span class="text-zinc-500">نجاح:</span> <span class="text-green-400">{lastReport.successful}</span></div>
          <div><span class="text-zinc-500">فشل:</span> <span class="text-red-400">{lastReport.failed}</span></div>
          <div><span class="text-zinc-500">معدل الخطأ:</span> <span class="text-orange-400">{(lastReport.error_rate * 100).toFixed(1)}%</span></div>
          <div><span class="text-zinc-500">avg latency:</span> <span class="text-blue-400">{lastReport.avg_latency_us}μs</span></div>
          <div><span class="text-zinc-500">مسارات:</span> <span class="text-white">{lastReport.tracked_paths}</span></div>
          <div><span class="text-zinc-500">قواعد تنبيه:</span> <span class="text-white">{lastReport.alert_rules}</span></div>
          <div><span class="text-zinc-500">تنبيهات:</span> <span class="text-yellow-400">{lastReport.recent_alerts}</span></div>
        </div>
      </div>
    {/if}
  </div>

  <!-- قائمة التقارير -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    <div class="px-5 py-3 border-b border-zinc-800">
      <h2 class="text-sm font-semibold text-white">التقارير المُولّدة</h2>
    </div>
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if reports.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد تقارير مُولّدة بعد</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each reports as r}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1">
                <div class="flex items-center gap-2">
                  <span class="px-2 py-0.5 text-xs bg-zinc-700 text-zinc-300 rounded-full">{periodLabel(r.period)}</span>
                  <span class="text-xs text-zinc-500">{formatTime(r.generated_at)}</span>
                </div>
                <p class="mt-1 text-sm text-zinc-400">{r.summary}</p>
                <div class="mt-1 flex gap-4 text-xs text-zinc-600">
                  <span>{r.total_requests} طلب</span>
                  <span class="text-green-500">{r.successful} نجح</span>
                  <span class="text-red-500">{r.failed} فشل</span>
                  <span>avg {r.avg_latency_us}μs</span>
                  <span>{(r.error_rate * 100).toFixed(1)}% خطأ</span>
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
