<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface SecurityStats {
    total_alerts: number;
    active_alerts: number;
    resolved_alerts: number;
    critical_alerts: number;
    high_alerts: number;
    last_alert_at: number | null;
  }

  interface SecurityAlert {
    id: string;
    actor: string;
    severity: 'info' | 'low' | 'medium' | 'high' | 'critical';
    threat_type: string;
    description: string;
    status: 'active' | 'investigating' | 'resolved' | 'dismissed';
    created_at: number;
  }

  let stats: SecurityStats | null = null;
  let alerts: SecurityAlert[] = [];
  let loading = true;
  let error = '';

  async function loadData() {
    loading = true;
    error = '';
    try {
      const [statsResp, alertsResp] = await Promise.all([
        api.request<SecurityStats>('/api/security/stats'),
        api.request<{ alerts: SecurityAlert[] }>('/api/security/alerts')
      ]);
      stats = statsResp;
      alerts = alertsResp.alerts || [];
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل البيانات';
    } finally {
      loading = false;
    }
  }

  async function resolveAlert(id: string) {
    try {
      await api.request(`/api/security/alerts/${id}/resolve`, { method: 'POST' });
      await loadData();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الحل';
    }
  }

  async function dismissAlert(id: string) {
    try {
      await api.request(`/api/security/alerts/${id}/dismiss`, { method: 'POST' });
      await loadData();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التجاهل';
    }
  }

  function formatTime(ns: number | null): string {
    if (!ns) return '—';
    return new Date(ns / 1_000_000).toLocaleString('ar');
  }

  function severityColor(s: string): string {
    const colors: Record<string, string> = {
      critical: 'bg-red-900 text-red-300',
      high: 'bg-orange-900 text-orange-300',
      medium: 'bg-yellow-900 text-yellow-300',
      low: 'bg-blue-900 text-blue-300',
      info: 'bg-zinc-700 text-zinc-300',
    };
    return colors[s] || 'bg-zinc-800 text-zinc-400';
  }

  onMount(loadData);
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">لوحة الأمان</h1>
      <p class="text-sm text-zinc-400 mt-1">كشف التهديدات والتنبيهات الأمنية</p>
    </div>
    <button onclick={loadData} class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition">
      تحديث
    </button>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-6 gap-3">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{stats.total_alerts}</div>
        <div class="text-xs text-zinc-500 mt-1">الإجمالي</div>
      </div>
      <div class="bg-red-900/30 border border-red-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-red-400">{stats.active_alerts}</div>
        <div class="text-xs text-zinc-500 mt-1">نشطة</div>
      </div>
      <div class="bg-green-900/30 border border-green-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-green-400">{stats.resolved_alerts}</div>
        <div class="text-xs text-zinc-500 mt-1">محلة</div>
      </div>
      <div class="bg-red-900/50 border border-red-700 rounded-xl p-4">
        <div class="text-2xl font-bold text-red-500">{stats.critical_alerts}</div>
        <div class="text-xs text-zinc-500 mt-1">حرجة</div>
      </div>
      <div class="bg-orange-900/50 border border-orange-700 rounded-xl p-4">
        <div class="text-2xl font-bold text-orange-500">{stats.high_alerts}</div>
        <div class="text-xs text-zinc-500 mt-1">عالية</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-xs text-zinc-500">آخر تنبيه</div>
        <div class="text-xs text-zinc-400 mt-1">{formatTime(stats.last_alert_at)}</div>
      </div>
    </div>
  {/if}

  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    <div class="px-5 py-3 border-b border-zinc-800">
      <h2 class="text-sm font-semibold text-white">التنبيهات الأمنية</h2>
    </div>
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if alerts.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد تنبيهات أمنية</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each alerts as a (a.id)}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class={`px-2 py-0.5 text-xs rounded-full font-mono ${severityColor(a.severity)}`}>
                    {a.severity}
                  </span>
                  <span class="text-sm font-medium text-white">{a.threat_type}</span>
                  {#if a.status === 'active'}
                    <span class="text-red-400 text-xs">● نشط</span>
                  {:else if a.status === 'resolved'}
                    <span class="text-green-400 text-xs">✓ محلول</span>
                  {/if}
                </div>
                <p class="mt-1 text-sm text-zinc-400">{a.description}</p>
                <div class="mt-1 text-xs text-zinc-600 flex gap-4">
                  <span>الفاعل: <code class="text-zinc-400">{a.actor}</code></span>
                  <span>{formatTime(a.created_at)}</span>
                </div>
              </div>
              {#if a.status === 'active'}
                <div class="flex gap-1 flex-shrink-0">
                  <button onclick={() => resolveAlert(a.id)} class="px-2 py-1 text-xs bg-green-700 hover:bg-green-600 text-white rounded transition">حل</button>
                  <button onclick={() => dismissAlert(a.id)} class="px-2 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded transition">تجاهل</button>
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
