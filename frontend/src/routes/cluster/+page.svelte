<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  interface ClusterNode {
    id: string;
    name: string;
    role: 'global' | 'regional' | 'edge' | 'local';
    region: string;
    addr: string;
    capabilities: string[];
    priority: number;
    status: 'healthy' | 'degraded' | 'unhealthy' | 'offline';
    registered_at: number;
    last_heartbeat: number;
    heartbeat_count: number;
    is_local: boolean;
  }

  interface ClusterStats {
    total_nodes: number;
    healthy_nodes: number;
    degraded_nodes: number;
    unhealthy_nodes: number;
    offline_nodes: number;
    by_role: Record<string, number>;
    by_region: Record<string, number>;
  }

  let nodes = $state<ClusterNode[]>([]);
  let stats = $state<ClusterStats | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // Register form
  let regName = $state('');
  let regRole = $state('regional');
  let regRegion = $state('eu-west-1');
  let regAddr = $state('0.0.0.0:4433');
  let regPriority = $state('10');
  let regLoading = $state(false);
  let regResult = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      const [nodesResp, statsResp] = await Promise.all([
        api.request<{ ok: boolean; nodes: ClusterNode[] }>('/api/cluster/nodes'),
        api.request<{ ok: boolean; stats: ClusterStats }>('/api/cluster/stats'),
      ]);
      nodes = nodesResp.nodes || [];
      stats = statsResp.stats;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  async function handleRegister(e: Event) {
    e.preventDefault();
    regLoading = true;
    regResult = null;
    try {
      const nodeId = `node-${Date.now().toString(36)}`;
      const resp = await api.request<{ ok: boolean }>('/api/cluster/nodes', {
        method: 'POST',
        body: JSON.stringify({
          id: nodeId,
          name: regName,
          role: regRole,
          region: regRegion,
          addr: regAddr,
          capabilities: [],
          priority: parseInt(regPriority) || 0,
          status: 'healthy',
          registered_at: 0,
          last_heartbeat: 0,
          heartbeat_count: 0,
          is_local: false,
        }),
      });
      if (resp.ok) {
        regResult = `Node "${regName}" registered`;
        regName = '';
        await load();
      } else {
        regResult = 'Failed to register node';
      }
    } catch (err) {
      regResult = `Error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      regLoading = false;
    }
  }

  async function handleHeartbeat(id: string) {
    try {
      await api.request(`/api/cluster/nodes/${encodeURIComponent(id)}/heartbeat`, { method: 'POST' });
      await load();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Heartbeat failed';
    }
  }

  function statusBadge(status: string): 'success' | 'warning' | 'error' | 'muted' {
    switch (status) {
      case 'healthy': return 'success';
      case 'degraded': return 'warning';
      case 'unhealthy': return 'error';
      default: return 'muted';
    }
  }

  function roleIcon(role: string): string {
    switch (role) {
      case 'global': return '🌍';
      case 'regional': return '🏛';
      case 'edge': return '⚡';
      case 'local': return '🖥';
      default: return '⚪';
    }
  }

  function timeAgo(ns: number): string {
    const ms = ns / 1_000_000;
    const diff = Date.now() - ms;
    const s = Math.floor(diff / 1000);
    if (s < 60) return `${s}s ago`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ago`;
    return `${Math.floor(m / 60)}h ago`;
  }

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Cluster</h1>
      <p class="text-sm text-nexora-muted">
        {nodes.length} nodes · {stats?.healthy_nodes || 0} healthy
      </p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>
      {loading ? 'Loading…' : '↻ Refresh'}
    </button>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">{error}</div>
  {/if}

  <!-- Stats grid -->
  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-5 gap-4 mb-8">
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Total</p>
        <p class="text-2xl font-semibold">{stats.total_nodes}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Healthy</p>
        <p class="text-2xl font-semibold text-emerald-400">{stats.healthy_nodes}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Degraded</p>
        <p class="text-2xl font-semibold text-amber-400">{stats.degraded_nodes}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Unhealthy</p>
        <p class="text-2xl font-semibold text-red-400">{stats.unhealthy_nodes}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Offline</p>
        <p class="text-2xl font-semibold text-zinc-500">{stats.offline_nodes}</p>
      </div>
    </div>

    <!-- By role + region -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-8">
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">By Role</h3>
        <div class="space-y-2">
          {#each Object.entries(stats.by_role) as [role, count]}
            <div class="flex items-center justify-between text-sm">
              <span class="flex items-center gap-2">
                <span>{roleIcon(role)}</span>
                <span class="capitalize">{role}</span>
              </span>
              <span class="font-mono font-semibold">{count}</span>
            </div>
          {/each}
          {#if Object.keys(stats.by_role).length === 0}
            <p class="text-sm text-nexora-muted">No nodes registered</p>
          {/if}
        </div>
      </div>
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">By Region</h3>
        <div class="space-y-2">
          {#each Object.entries(stats.by_region) as [region, count]}
            <div class="flex items-center justify-between text-sm">
              <span class="font-mono">{region}</span>
              <span class="font-mono font-semibold">{count}</span>
            </div>
          {/each}
          {#if Object.keys(stats.by_region).length === 0}
            <p class="text-sm text-nexora-muted">No nodes registered</p>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- Register new node -->
  <div class="card mb-6">
    <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Register New Node</h3>
    <form onsubmit={handleRegister} class="grid grid-cols-1 md:grid-cols-3 gap-3">
      <input class="input" placeholder="Node name" bind:value={regName} disabled={regLoading} />
      <select class="input" bind:value={regRole} disabled={regLoading}>
        <option value="global">Global</option>
        <option value="regional">Regional</option>
        <option value="edge">Edge</option>
        <option value="local">Local</option>
      </select>
      <input class="input" placeholder="Region (e.g. eu-west-1)" bind:value={regRegion} disabled={regLoading} />
      <input class="input" placeholder="Address (e.g. 10.0.0.5:4433)" bind:value={regAddr} disabled={regLoading} />
      <input class="input" type="number" placeholder="Priority" bind:value={regPriority} disabled={regLoading} />
      <button type="submit" class="btn-primary" disabled={regLoading || !regName}>
        {regLoading ? 'Registering…' : 'Register Node'}
      </button>
    </form>
    {#if regResult}
      <div class="mt-3 p-2 rounded-md bg-nexora-bg border border-nexora-border text-xs font-mono {regResult.startsWith('Error') ? 'text-red-400' : 'text-emerald-400'}">
        {regResult}
      </div>
    {/if}
  </div>

  <!-- Nodes grid -->
  {#if nodes.length === 0 && !loading}
    <div class="card text-center text-nexora-muted">
      No nodes registered. Register one above to start building your cluster.
    </div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {#each nodes as node}
        <div class="card">
          <div class="flex items-start justify-between mb-3">
            <div class="flex items-center gap-2">
              <span class="text-2xl">{roleIcon(node.role)}</span>
              <div>
                <h3 class="font-semibold">{node.name}</h3>
                <p class="text-xs text-nexora-muted font-mono">{node.id.slice(0, 12)}…</p>
              </div>
            </div>
            <span class="badge-{statusBadge(node.status)}">{node.status}</span>
          </div>

          <dl class="space-y-2 text-xs">
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Role</dt>
              <dd class="capitalize">{node.role}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Region</dt>
              <dd class="font-mono">{node.region}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Address</dt>
              <dd class="font-mono">{node.addr}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Priority</dt>
              <dd class="font-mono">{node.priority}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Heartbeats</dt>
              <dd class="font-mono">{node.heartbeat_count}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Last heartbeat</dt>
              <dd>{timeAgo(node.last_heartbeat)}</dd>
            </div>
            {#if node.is_local}
              <div class="mt-2">
                <span class="badge-success">local node</span>
              </div>
            {/if}
          </dl>

          <div class="mt-4 pt-3 border-t border-nexora-border">
            <button
              class="btn-ghost w-full text-xs"
              onclick={() => handleHeartbeat(node.id)}
              disabled={node.status === 'offline'}
            >
              Send Heartbeat
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</Layout>
