<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface TraceSummary {
    trace_id: string;
    span_count: number;
    root_name: string;
    total_duration_ms: number;
  }

  interface Span {
    span_id: string;
    name: string;
    kind: string;
    status: string;
    start_time: number;
    duration_nanos: number | null;
    duration_ms: number | null;
    attributes: Record<string, string>;
    events: { name: string; timestamp: number }[];
  }

  interface TraceDetail {
    trace_id: string;
    spans: Span[];
    span_count: number;
  }

  let traces: TraceSummary[] = [];
  let totalTraces = 0;
  let totalSpans = 0;
  let loading = true;
  let error = '';
  let selectedTrace: TraceDetail | null = null;
  let autoRefresh = true;
  let interval: ReturnType<typeof setInterval>;

  async function loadTraces() {
    loading = true;
    error = '';
    try {
      const resp = await api.request<{
        traces: TraceSummary[];
        count: number;
        total_traces: number;
        total_spans: number;
      }>('/api/tracing/recent');
      traces = resp.traces || [];
      totalTraces = resp.total_traces || 0;
      totalSpans = resp.total_spans || 0;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التحميل';
    } finally {
      loading = false;
    }
  }

  async function viewTrace(traceId: string) {
    error = '';
    try {
      const resp = await api.request<TraceDetail>(`/api/tracing/${traceId}`);
      selectedTrace = resp;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل التتبع';
    }
  }

  function closeTrace() {
    selectedTrace = null;
  }

  function formatTime(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleTimeString('ar');
  }

  function formatDuration(ms: number | null): string {
    if (ms === null) return '—';
    if (ms < 1) return `${(ms * 1000).toFixed(0)}μs`;
    return `${ms.toFixed(2)}ms`;
  }

  function statusColor(s: string): string {
    return { ok: 'text-green-400', error: 'text-red-400', active: 'text-blue-400' }[s] || 'text-zinc-400';
  }

  function kindColor(k: string): string {
    return {
      server: 'bg-blue-900 text-blue-300',
      client: 'bg-purple-900 text-purple-300',
      internal: 'bg-zinc-700 text-zinc-300',
      producer: 'bg-green-900 text-green-300',
      consumer: 'bg-yellow-900 text-yellow-300',
    }[k] || 'bg-zinc-800 text-zinc-400';
  }

  onMount(() => {
    loadTraces();
    interval = setInterval(() => { if (autoRefresh && !selectedTrace) loadTraces(); }, 5000);
  });
  onDestroy(() => clearInterval(interval));
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">التتبع الموزّع</h1>
      <p class="text-sm text-zinc-400 mt-1">{totalTraces} تتبع · {totalSpans} span</p>
    </div>
    <div class="flex gap-2 items-center">
      <span class="flex items-center gap-2 text-sm text-zinc-400">
        <input type="checkbox" bind:checked={autoRefresh} class="rounded" />
        تحديث تلقائي
      </span>
      <button onclick={loadTraces} class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition">تحديث</button>
    </div>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if selectedTrace}
    <!-- تفاصيل تتبع محدد -->
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
      <div class="px-5 py-3 border-b border-zinc-800 flex items-center justify-between">
        <div>
          <h2 class="text-sm font-semibold text-white">تتبع: <code class="text-blue-400">{selectedTrace.trace_id}</code></h2>
          <p class="text-xs text-zinc-500 mt-1">{selectedTrace.span_count} span</p>
        </div>
        <button onclick={closeTrace} class="px-3 py-1 bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded-lg text-sm transition">← رجوع</button>
      </div>
      <div class="divide-y divide-zinc-800">
        {#each selectedTrace.spans as s}
          <div class="p-4">
            <div class="flex items-center gap-2 flex-wrap mb-2">
              <span class={`px-2 py-0.5 text-xs rounded-full font-mono ${kindColor(s.kind)}`}>{s.kind}</span>
              <code class="text-sm text-zinc-300">{s.name}</code>
              <span class={`text-xs ${statusColor(s.status)}`}>● {s.status}</span>
              <span class="text-xs text-zinc-500 ml-auto">{formatDuration(s.duration_ms)}</span>
            </div>
            <div class="text-xs text-zinc-600 flex gap-4">
              <span>ID: <code class="text-zinc-500">{s.span_id}</code></span>
              <span>الوقت: {formatTime(s.start_time)}</span>
            </div>
            {#if Object.keys(s.attributes || {}).length > 0}
              <div class="mt-2 flex gap-2 flex-wrap">
                {#each Object.entries(s.attributes) as [k, v]}
                  <span class="text-xs bg-zinc-800 px-2 py-0.5 rounded text-zinc-400">{k}: {v}</span>
                {/each}
              </div>
            {/if}
            {#if s.events && s.events.length > 0}
              <div class="mt-2">
                {#each s.events as ev}
                  <div class="text-xs text-yellow-500">⚡ {ev.name}</div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {:else if loading}
    <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
  {:else if traces.length === 0}
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-8 text-center text-zinc-500">
      لا توجد تتبعات. كل طلب HTTP يُنشئ تتبعاً تلقائياً.
    </div>
  {:else}
    <!-- قائمة التتبعات -->
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
      <div class="divide-y divide-zinc-800">
        {#each traces as t}
          <button
            onclick={() => viewTrace(t.trace_id)}
            class="w-full p-4 hover:bg-zinc-800/50 transition text-left"
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <code class="text-xs text-blue-400 font-mono">{t.trace_id.substring(0, 16)}...</code>
                <span class="text-sm text-zinc-300">{t.root_name}</span>
                <span class="text-xs text-zinc-600">{t.span_count} span</span>
              </div>
              <div class="flex items-center gap-3">
                <span class="text-sm text-zinc-400">{t.total_duration_ms.toFixed(1)}ms</span>
                <span class="text-zinc-600">→</span>
              </div>
            </div>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>
