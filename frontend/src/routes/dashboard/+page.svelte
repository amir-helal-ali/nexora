<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api/gateway';

  let snapshot: any = null;
  let alerts: any[] = [];
  let securityStats: any = null;
  let auditStats: any = null;
  let loading = true;
  let error = '';
  let autoRefresh = true;
  let interval: ReturnType<typeof setInterval>;

  async function loadAll() {
    error = '';
    try {
      const [snap, alertsResp, secStats, audStats] = await Promise.all([
        api.request('/api/monitoring/snapshot'),
        api.request('/api/monitoring/alerts'),
        api.request('/api/security/stats'),
        api.request('/api/audit/stats')
      ]);
      snapshot = snap;
      alerts = alertsResp.active_alerts || [];
      securityStats = secStats;
      auditStats = audStats;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التحميل';
    } finally {
      loading = false;
    }
  }

  function healthColor(s: string): string {
    return { healthy: 'text-green-400', degraded: 'text-yellow-400', unhealthy: 'text-red-400' }[s] || 'text-zinc-400';
  }

  function alertColor(l: string): string {
    return { info: 'text-blue-400', warning: 'text-yellow-400', critical: 'text-red-400' }[l] || 'text-zinc-400';
  }

  function formatLatency(us: number): string {
    if (us < 1000) return `${us}μs`;
    return `${(us / 1000).toFixed(1)}ms`;
  }

  onMount(() => {
    loadAll();
    interval = setInterval(() => { if (autoRefresh) loadAll(); }, 5000);
  });
  onDestroy(() => clearInterval(interval));
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">لوحة التحكم التنفيذية</h1>
      <p class="text-sm text-zinc-400 mt-1">نظرة شاملة على المنصة</p>
    </div>
    <span class="flex items-center gap-2 text-sm text-zinc-400">
      <input type="checkbox" bind:checked={autoRefresh} class="rounded" />
      تحديث تلقائي
    </span>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if loading}
    <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
  {:else}
    <!-- الصف 1: الحالة الإجمالية -->
    <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <div class="text-xs text-zinc-500 mb-2">صحة النظام</div>
        <div class={`text-2xl font-bold ${healthColor(snapshot?.overall_health || 'healthy')}`}>
          {snapshot?.overall_health === 'healthy' ? '✓ صحّي' : snapshot?.overall_health === 'degraded' ? '⚠ متدهور' : '✕ غير صحّي'}
        </div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <div class="text-xs text-zinc-500 mb-2">إجمالي الطلبات</div>
        <div class="text-2xl font-bold text-white">{snapshot?.total_requests ?? 0}</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <div class="text-xs text-zinc-500 mb-2">معدل الخطأ</div>
        <div class={`text-2xl font-bold ${(snapshot?.error_rate ?? 0) > 0.05 ? 'text-red-400' : 'text-green-400'}`}>
          {((snapshot?.error_rate ?? 0) * 100).toFixed(1)}%
        </div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <div class="text-xs text-zinc-500 mb-2">متوسط الزمن</div>
        <div class="text-2xl font-bold text-blue-400">{formatLatency(snapshot?.avg_latency_us ?? 0)}</div>
      </div>
    </div>

    <!-- الصف 2: الأمان + التدقيق -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">الأمان</h3>
        <div class="space-y-2 text-sm">
          <div class="flex justify-between">
            <span class="text-zinc-500">تنبيهات نشطة</span>
            <span class="text-red-400">{securityStats?.active_alerts ?? 0}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-zinc-500">تنبيهات حرجة</span>
            <span class="text-red-500">{securityStats?.critical_alerts ?? 0}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-zinc-500">إجمالي التنبيهات</span>
            <span class="text-white">{securityStats?.total_alerts ?? 0}</span>
          </div>
        </div>
      </div>

      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">التدقيق</h3>
        <div class="space-y-2 text-sm">
          <div class="flex justify-between">
            <span class="text-zinc-500">مدخلات ناجحة</span>
            <span class="text-green-400">{auditStats?.success ?? 0}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-zinc-500">مدخلات فاشلة</span>
            <span class="text-red-400">{auditStats?.failure ?? 0}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-zinc-500">الإجمالي</span>
            <span class="text-white">{auditStats?.total ?? 0}</span>
          </div>
        </div>
      </div>

      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">الأداء</h3>
        <div class="space-y-2 text-sm">
          <div class="flex justify-between">
            <span class="text-zinc-500">طلبات ناجحة</span>
            <span class="text-green-400">{snapshot?.successful ?? 0}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-zinc-500">طلبات فاشلة</span>
            <span class="text-red-400">{snapshot?.failed ?? 0}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-zinc-500">مسارات متتبّعة</span>
            <span class="text-white">{snapshot?.tracked_paths ?? 0}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- الصف 3: تنبيهات الأداء -->
    {#if alerts.length > 0}
      <div class="bg-red-900/30 border border-red-700 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-red-300 mb-3">⚠ تنبيهات أداء نشطة ({alerts.length})</h3>
        <div class="space-y-2">
          {#each alerts as a}
            <div class="flex items-center justify-between text-sm">
              <div>
                <span class={`font-mono ${alertColor(a.level)}`}>[{a.level}]</span>
                <span class="text-zinc-300 ml-2">{a.rule_name}</span>
              </div>
              <span class="text-zinc-500 text-xs">{a.message}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- الصف 4: أعلى المسارات بطئاً + أخطاءً -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">أعلى المسارات بطئاً</h3>
        {#if snapshot?.slowest_paths?.length > 0}
          <div class="space-y-2">
            {#each snapshot.slowest_paths as [path, latency]}
              <div class="flex items-center justify-between text-sm">
                <code class="text-xs text-zinc-400 truncate">{path}</code>
                <span class="text-orange-400 font-mono">{formatLatency(latency)}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-sm text-zinc-600">لا توجد بيانات</p>
        {/if}
      </div>

      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
        <h3 class="text-sm font-semibold text-white mb-3">أكثر المسارات أخطاءً</h3>
        {#if snapshot?.error_paths?.length > 0}
          <div class="space-y-2">
            {#each snapshot.error_paths as [path, count]}
              <div class="flex items-center justify-between text-sm">
                <code class="text-xs text-zinc-400 truncate">{path}</code>
                <span class="text-red-400 font-mono">{count}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-sm text-green-400">✓ لا توجد أخطاء</p>
        {/if}
      </div>
    </div>
  {/if}
</div>
