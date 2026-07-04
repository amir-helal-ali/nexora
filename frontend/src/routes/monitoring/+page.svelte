<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface Snapshot {
    overall_health: string;
    total_requests: number;
    successful: number;
    failed: number;
    avg_latency_us: number;
    error_rate: number;
    tracked_paths: number;
    probe_count: number;
    slowest_paths: [string, number][];
    error_paths: [string, number][];
  }

  interface PathMetric {
    path: string;
    total_requests: number;
    successful: number;
    failed: number;
    avg_latency_us: number;
    min_latency_us: number;
    max_latency_us: number;
    error_rate: number;
  }

  interface Probe {
    name: string;
    status: string;
    message: string;
    checked_at: number;
    duration_us: number;
  }

  let snapshot: Snapshot | null = null;
  let paths: PathMetric[] = [];
  let probes: Probe[] = [];
  let overallStatus = '';
  let loading = true;
  let error = '';
  let autoRefresh = true;
  let refreshInterval: ReturnType<typeof setInterval>;

  async function loadAll() {
    error = '';
    try {
      const [snap, pathsResp, healthResp] = await Promise.all([
        api.request<Snapshot>('/api/monitoring/snapshot'),
        api.request<{ paths: PathMetric[] }>('/api/monitoring/paths'),
        api.request<{ overall_status: string; probes: Probe[] }>('/api/monitoring/health')
      ]);
      snapshot = snap;
      paths = pathsResp.paths || [];
      probes = healthResp.probes || [];
      overallStatus = healthResp.overall_status;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل المراقبة';
    } finally {
      loading = false;
    }
  }

  async function resetMetrics() {
    if (!confirm('إعادة ضبط كل المقاييس؟')) return;
    try {
      await api.request('/api/monitoring/reset', { method: 'POST' });
      await loadAll();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الإعادة';
    }
  }

  function healthColor(s: string): string {
    return { healthy: 'text-green-400', degraded: 'text-yellow-400', unhealthy: 'text-red-400' }[s] || 'text-zinc-400';
  }

  function healthBg(s: string): string {
    return {
      healthy: 'bg-green-900/30 border-green-700',
      degraded: 'bg-yellow-900/30 border-yellow-700',
      unhealthy: 'bg-red-900/30 border-red-700'
    }[s] || 'bg-zinc-800 border-zinc-700';
  }

  function formatLatency(us: number): string {
    if (us < 1000) return `${us}μs`;
    return `${(us / 1000).toFixed(1)}ms`;
  }

  function formatTime(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleTimeString('ar');
  }

  onMount(() => {
    loadAll();
    refreshInterval = setInterval(() => {
      if (autoRefresh) loadAll();
    }, 5000);
  });

  onDestroy(() => clearInterval(refreshInterval));
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">لوحة المراقبة</h1>
      <p class="text-sm text-zinc-400 mt-1">مقاييس الأداء + فحوصات الصحة</p>
    </div>
    <div class="flex gap-2 items-center">
      <span class="flex items-center gap-2 text-sm text-zinc-400">
        <input type="checkbox" bind:checked={autoRefresh} class="rounded" />
        تحديث تلقائي (5ث)
      </span>
      <button onclick={loadAll} class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition">تحديث</button>
      <button onclick={resetMetrics} class="px-4 py-2 bg-red-900/50 hover:bg-red-800 text-red-200 rounded-lg text-sm transition">إعادة ضبط</button>
    </div>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if loading}
    <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
  {:else if snapshot}
    <!-- الحالة الإجمالية -->
    <div class={`border rounded-xl p-4 ${healthBg(overallStatus)}`}>
      <div class="flex items-center gap-3">
        <span class={`text-2xl font-bold ${healthColor(overallStatus)}`}>
          {overallStatus === 'healthy' ? '✓' : overallStatus === 'degraded' ? '⚠' : '✕'}
        </span>
        <div>
          <span class={`text-sm font-semibold ${healthColor(overallStatus)}`}>
            {overallStatus === 'healthy' ? 'النظام صحّي' : overallStatus === 'degraded' ? 'متدهور' : 'غير صحّي'}
          </span>
          <span class="text-xs text-zinc-500 ml-2">{snapshot.probe_count} فحص صحة</span>
        </div>
      </div>
    </div>

    <!-- بطاقات الإحصائيات -->
    <div class="grid grid-cols-2 md:grid-cols-5 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{snapshot.total_requests}</div>
        <div class="text-xs text-zinc-500 mt-1">إجمالي الطلبات</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-green-400">{snapshot.successful}</div>
        <div class="text-xs text-zinc-500 mt-1">ناجحة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-red-400">{snapshot.failed}</div>
        <div class="text-xs text-zinc-500 mt-1">فاشلة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-blue-400">{formatLatency(snapshot.avg_latency_us)}</div>
        <div class="text-xs text-zinc-500 mt-1">متوسط الزمن</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-orange-400">{(snapshot.error_rate * 100).toFixed(1)}%</div>
        <div class="text-xs text-zinc-500 mt-1">معدل الخطأ</div>
      </div>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <!-- فحوصات الصحة -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-5 py-3 border-b border-zinc-800">
          <h2 class="text-sm font-semibold text-white">فحوصات الصحة ({probes.length})</h2>
        </div>
        {#if probes.length === 0}
          <div class="p-4 text-center text-zinc-500 text-sm">لا توجد فحوصات مسجّلة</div>
        {:else}
          <div class="divide-y divide-zinc-800">
            {#each probes as p}
              <div class="p-3 flex items-center justify-between">
                <div>
                  <code class="text-sm text-zinc-300">{p.name}</code>
                  <p class="text-xs text-zinc-600 mt-0.5">{p.message}</p>
                </div>
                <span class={`px-2 py-0.5 text-xs rounded-full ${healthBg(p.status)} ${healthColor(p.status)}`}>
                  {p.status}
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- أعلى المسارات بطئاً -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-5 py-3 border-b border-zinc-800">
          <h2 class="text-sm font-semibold text-white">أعلى المسارات بطئاً</h2>
        </div>
        {#if snapshot.slowest_paths.length === 0}
          <div class="p-4 text-center text-zinc-500 text-sm">لا توجد بيانات</div>
        {:else}
          <div class="divide-y divide-zinc-800">
            {#each snapshot.slowest_paths as [path, latency]}
              <div class="p-3 flex items-center justify-between">
                <code class="text-xs text-zinc-400 truncate">{path}</code>
                <span class="text-sm text-orange-400 font-mono">{formatLatency(latency)}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- أكثر المسارات أخطاءً -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-5 py-3 border-b border-zinc-800">
          <h2 class="text-sm font-semibold text-white">أكثر المسارات أخطاءً</h2>
        </div>
        {#if snapshot.error_paths.length === 0}
          <div class="p-4 text-center text-zinc-500 text-sm">لا توجد أخطاء ✅</div>
        {:else}
          <div class="divide-y divide-zinc-800">
            {#each snapshot.error_paths as [path, count]}
              <div class="p-3 flex items-center justify-between">
                <code class="text-xs text-zinc-400 truncate">{path}</code>
                <span class="text-sm text-red-400 font-mono">{count} خطأ</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- مقاييس كل مسار -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-5 py-3 border-b border-zinc-800">
          <h2 class="text-sm font-semibold text-white">مقاييس المسارات ({paths.length})</h2>
        </div>
        {#if paths.length === 0}
          <div class="p-4 text-center text-zinc-500 text-sm">لا توجد مسارات متتبّعة</div>
        {:else}
          <div class="divide-y divide-zinc-800 max-h-64 overflow-y-auto">
            {#each paths as p}
              <div class="p-3">
                <div class="flex items-center justify-between">
                  <code class="text-xs text-zinc-400 truncate">{p.path}</code>
                  <span class={`text-xs ${p.error_rate > 0.1 ? 'text-red-400' : 'text-green-400'}`}>
                    {(p.error_rate * 100).toFixed(0)}% خطأ
                  </span>
                </div>
                <div class="mt-1 flex gap-3 text-xs text-zinc-600">
                  <span>{p.total_requests} طلب</span>
                  <span>avg: {formatLatency(p.avg_latency_us)}</span>
                  <span>min: {formatLatency(p.min_latency_us)}</span>
                  <span>max: {formatLatency(p.max_latency_us)}</span>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
