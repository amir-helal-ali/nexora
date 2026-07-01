<script lang="ts">
  import { onMount } from 'svelte';
  import Layout from '$lib/components/Layout.svelte';

  interface Endpoint {
    method: 'GET' | 'POST' | 'DELETE';
    path: string;
    description: string;
    requiresAuth: boolean;
    bodyExample?: string;
    category: string;
  }

  const endpoints: Endpoint[] = [
    { method: 'GET', path: '/api/health', description: 'Gateway liveness', requiresAuth: false, category: 'System' },
    { method: 'GET', path: '/api/openapi.json', description: 'OpenAPI spec', requiresAuth: false, category: 'System' },
    { method: 'POST', path: '/api/auth/login', description: 'Login', requiresAuth: false, bodyExample: '{"username":"admin","password":"admin123"}', category: 'Auth' },
    { method: 'POST', path: '/api/auth/logout', description: 'Logout', requiresAuth: true, bodyExample: '{"token":"<token>"}', category: 'Auth' },
    { method: 'POST', path: '/api/core/ping', description: 'Ping Core', requiresAuth: true, category: 'Core' },
    { method: 'GET', path: '/api/core/events', description: 'Replay events', requiresAuth: true, category: 'Core' },
    { method: 'POST', path: '/api/core/events', description: 'Publish event', requiresAuth: true, bodyExample: '{"name":"test.event","payload":"hello"}', category: 'Core' },
    { method: 'GET', path: '/api/core/modules', description: 'List modules', requiresAuth: true, category: 'Core' },
    { method: 'GET', path: '/api/core/health', description: 'Core health', requiresAuth: true, category: 'Core' },
    { method: 'GET', path: '/api/marketplace/packages', description: 'List packages', requiresAuth: true, category: 'Marketplace' },
    { method: 'GET', path: '/api/marketplace/installed', description: 'Installed packages', requiresAuth: true, category: 'Marketplace' },
    { method: 'GET', path: '/api/marketplace/updates/check', description: 'Check updates', requiresAuth: true, category: 'Marketplace' },
    { method: 'GET', path: '/api/billing/invoices', description: 'List invoices', requiresAuth: true, category: 'Billing' },
    { method: 'POST', path: '/api/billing/invoices', description: 'Create invoice', requiresAuth: true, bodyExample: '{"customer_id":"admin","customer_name":"Admin","currency":"USD","items":[{"description":"Pro","quantity":1,"unit_price_minor":1999}]}', category: 'Billing' },
    { method: 'GET', path: '/api/billing/stats', description: 'Billing stats', requiresAuth: true, category: 'Billing' },
    { method: 'GET', path: '/api/workflows', description: 'List workflows', requiresAuth: true, category: 'Workflow' },
    { method: 'GET', path: '/api/workflows/stats', description: 'Workflow stats', requiresAuth: true, category: 'Workflow' },
    { method: 'GET', path: '/api/cluster/nodes', description: 'List nodes', requiresAuth: true, category: 'Cluster' },
    { method: 'GET', path: '/api/cluster/stats', description: 'Cluster stats', requiresAuth: true, category: 'Cluster' },
    { method: 'GET', path: '/api/notifications', description: 'Your notifications', requiresAuth: true, category: 'Notifications' },
    { method: 'GET', path: '/api/notifications/unread_count', description: 'Unread count', requiresAuth: true, category: 'Notifications' },
    { method: 'GET', path: '/api/users', description: 'List users', requiresAuth: true, category: 'Users' },
    { method: 'GET', path: '/api/users/me', description: 'Your profile', requiresAuth: true, category: 'Users' },
    { method: 'GET', path: '/api/dashboard/stats', description: 'Unified stats', requiresAuth: true, category: 'Dashboard' },
  ];

  let selected = $state<Endpoint | null>(null);
  let pathParams = $state<Record<string, string>>({});
  let queryStr = $state('');
  let reqBody = $state('');
  let respStatus = $state<number | null>(null);
  let respBody = $state('');
  let respTime = $state<number | null>(null);
  let sending = $state(false);
  let search = $state('');

  const cats = ['System', 'Auth', 'Core', 'Marketplace', 'Billing', 'Workflow', 'Cluster', 'Notifications', 'Users', 'Dashboard'];

  function mc(m: string): string {
    return m === 'GET' ? 'text-emerald-400' : m === 'POST' ? 'text-blue-400' : 'text-red-400';
  }

  function sel(ep: Endpoint) {
    selected = ep;
    pathParams = {};
    const m = ep.path.match(/:(\w+)/g);
    if (m) for (const p of m) pathParams[p.slice(1)] = '';
    reqBody = ep.bodyExample || '';
    respBody = ''; respStatus = null; respTime = null; queryStr = '';
  }

  async function send() {
    if (!selected) return;
    sending = true; respBody = ''; respStatus = null; respTime = null;
    let path = selected.path;
    for (const [k, v] of Object.entries(pathParams)) path = path.replace(`:${k}`, encodeURIComponent(v));
    if (queryStr) path += `?${queryStr}`;
    const t0 = performance.now();
    try {
      const token = localStorage.getItem('nexora.token');
      const h: Record<string, string> = { 'Content-Type': 'application/json' };
      if (selected.requiresAuth && token) h['Authorization'] = `Bearer ${token}`;
      const r = await fetch(path, { method: selected.method, headers: h, body: selected.method !== 'GET' && reqBody ? reqBody : undefined });
      respStatus = r.status; respTime = Math.round(performance.now() - t0);
      const txt = await r.text();
      try { respBody = JSON.stringify(JSON.parse(txt), null, 2); } catch { respBody = txt || '(empty)'; }
    } catch (e) { respStatus = 0; respTime = Math.round(performance.now() - t0); respBody = `Error: ${e}`; }
    finally { sending = false; }
  }

  function sc(s: number): string {
    return s >= 200 && s < 300 ? 'text-emerald-400' : s >= 400 && s < 500 ? 'text-amber-400' : s >= 500 ? 'text-red-400' : 'text-nexora-muted';
  }

  let filtered = $derived(search.trim() === '' ? endpoints : endpoints.filter(e => e.path.toLowerCase().includes(search.toLowerCase()) || e.description.toLowerCase().includes(search.toLowerCase())));

  onMount(() => { if (endpoints.length > 0) sel(endpoints[2]); }); // Select login by default
</script>

<Layout>
  <div class="mb-6">
    <h1 class="text-2xl font-semibold mb-1">API Explorer</h1>
    <p class="text-sm text-nexora-muted">{endpoints.length} endpoints · Test any API directly</p>
  </div>

  <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
    <!-- Endpoint list -->
    <div class="lg:col-span-1">
      <input class="input mb-3" placeholder="Search endpoints…" bind:value={search} />
      <div class="card !p-3 max-h-[600px] overflow-y-auto">
        {#each cats as cat}
          {@const eps = filtered.filter(e => e.category === cat)}
          {#if eps.length > 0}
            <div class="mb-3">
              <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-2 px-1">{cat}</h3>
              {#each eps as ep}
                <button class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-xs transition-colors text-left {selected?.path === ep.path && selected?.method === ep.method ? 'bg-nexora-accent/20 border border-nexora-accent/50' : 'hover:bg-nexora-border/50'}" onclick={() => sel(ep)}>
                  <span class="font-mono font-bold w-12 shrink-0 {mc(ep.method)}">{ep.method}</span>
                  <span class="font-mono truncate">{ep.path}</span>
                </button>
              {/each}
            </div>
          {/if}
        {/each}
      </div>
    </div>

    <!-- Request + Response -->
    <div class="lg:col-span-2 space-y-4">
      {#if selected}
        <div class="card">
          <div class="flex items-center gap-3 mb-3">
            <span class="font-mono font-bold text-lg {mc(selected.method)}">{selected.method}</span>
            <span class="font-mono text-sm flex-1">{selected.path}</span>
            {#if selected.requiresAuth}<span class="badge-muted">auth</span>{/if}
          </div>
          <p class="text-xs text-nexora-muted mb-4">{selected.description}</p>

          {#if Object.keys(pathParams).length > 0}
            <div class="mb-3">
              <label class="block text-xs text-nexora-muted mb-2">Path Parameters</label>
              {#each Object.keys(pathParams) as key}
                <div class="flex items-center gap-2 mb-2">
                  <span class="text-xs font-mono text-nexora-muted w-20">:{key}</span>
                  <input class="input text-xs" placeholder={key} bind:value={pathParams[key]} />
                </div>
              {/each}
            </div>
          {/if}

          {#if selected.method === 'GET'}
            <div class="mb-3">
              <label class="block text-xs text-nexora-muted mb-1">Query (optional)</label>
              <input class="input text-xs font-mono" placeholder="from_id=0&filter=user." bind:value={queryStr} />
            </div>
          {/if}

          {#if selected.method !== 'GET'}
            <div class="mb-3">
              <label class="block text-xs text-nexora-muted mb-1">Request Body (JSON)</label>
              <textarea class="input font-mono text-xs h-32 resize-y" placeholder="JSON body" bind:value={reqBody}></textarea>
            </div>
          {/if}

          <button class="btn-primary w-full" onclick={send} disabled={sending}>
            {sending ? 'Sending…' : 'Send Request'}
          </button>
        </div>

        {#if respStatus !== null}
          <div class="card">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted">Response</h3>
              <div class="flex items-center gap-3 text-xs">
                <span class={sc(respStatus)}>{respStatus === 0 ? 'Network Error' : respStatus}</span>
                {#if respTime !== null}<span class="text-nexora-muted">{respTime}ms</span>{/if}
              </div>
            </div>
            <pre class="bg-nexora-bg border border-nexora-border rounded-md p-4 text-xs font-mono overflow-x-auto max-h-[400px] overflow-y-auto">{respBody}</pre>
          </div>
        {/if}
      {:else}
        <div class="card text-center text-nexora-muted">Select an endpoint to start.</div>
      {/if}
    </div>
  </div>
</Layout>
