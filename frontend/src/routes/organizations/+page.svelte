<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  interface Organization {
    id: string;
    name: string;
    slug: string;
    tier: string;
    owner_id: string;
    description: string;
    active: boolean;
    created_at: number;
    max_members: number;
  }

  interface Membership {
    org_id: string;
    user_id: string;
    role: string;
    joined_at: number;
  }

  let orgs = $state<Organization[]>([]);
  let selectedOrg = $state<Organization | null>(null);
  let members = $state<Membership[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let newName = $state('');
  let newSlug = $state('');
  let newTier = $state('team');
  let createLoading = $state(false);
  let createResult = $state<string | null>(null);

  let newMemberId = $state('');
  let newMemberRole = $state('member');

  async function load() {
    loading = true;
    error = null;
    try {
      const resp = await api.request<{ ok: boolean; organizations: Organization[] }>('/api/tenancy/orgs');
      orgs = resp.organizations || [];
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  async function selectOrg(org: Organization) {
    selectedOrg = org;
    try {
      const resp = await api.request<{ ok: boolean; members: Membership[] }>(`/api/tenancy/orgs/${encodeURIComponent(org.id)}/members`);
      members = resp.members || [];
    } catch { members = []; }
  }

  async function handleCreate(e: Event) {
    e.preventDefault();
    createLoading = true;
    createResult = null;
    try {
      const resp = await api.request<{ ok: boolean }>('/api/tenancy/orgs', {
        method: 'POST',
        body: JSON.stringify({ name: newName, slug: newSlug, tier: newTier }),
      });
      if (resp.ok) {
        createResult = `Organization "${newName}" created`;
        newName = ''; newSlug = '';
        await load();
      } else {
        createResult = 'Failed to create organization';
      }
    } catch (err) {
      createResult = `Error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      createLoading = false;
    }
  }

  async function handleAddMember() {
    if (!selectedOrg || !newMemberId) return;
    try {
      await api.request(`/api/tenancy/orgs/${encodeURIComponent(selectedOrg.id)}/members`, {
        method: 'POST',
        body: JSON.stringify({ user_id: newMemberId, role: newMemberRole }),
      });
      newMemberId = '';
      await selectOrg(selectedOrg);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Add member failed';
    }
  }

  function tierBadge(tier: string): string {
    const map: Record<string, string> = {
      individual: 'muted', team: 'muted', organization: 'success', enterprise: 'warning', msp: 'error',
    };
    return map[tier] || 'muted';
  }

  function formatDate(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleDateString();
  }

  onMount(() => { load(); });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Organizations</h1>
      <p class="text-sm text-nexora-muted">{orgs.length} organizations · Multi-tenancy (Part 2 Law 23)</p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>{loading ? 'Loading…' : '↻ Refresh'}</button>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">{error}</div>
  {/if}

  <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
    <!-- Left: Org list + create form -->
    <div class="space-y-4">
      <div class="card">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Create Organization</h3>
        <form onsubmit={handleCreate} class="space-y-3">
          <input class="input text-sm" placeholder="Name" bind:value={newName} disabled={createLoading} />
          <input class="input text-sm" placeholder="Slug (e.g. acme-inc)" bind:value={newSlug} disabled={createLoading} />
          <select class="input text-sm" bind:value={newTier} disabled={createLoading}>
            <option value="individual">Individual</option>
            <option value="team">Team</option>
            <option value="organization">Organization</option>
            <option value="enterprise">Enterprise</option>
            <option value="msp">MSP</option>
          </select>
          <button type="submit" class="btn-primary w-full text-sm" disabled={createLoading || !newName || !newSlug}>
            {createLoading ? 'Creating…' : 'Create'}
          </button>
        </form>
        {#if createResult}
          <div class="mt-2 p-2 rounded text-xs font-mono {createResult.startsWith('Error') ? 'text-red-400' : 'text-emerald-400'}">{createResult}</div>
        {/if}
      </div>

      <div class="card !p-3">
        <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-2 px-1">Organizations</h3>
        {#if orgs.length === 0}
          <p class="text-sm text-nexora-muted px-1 py-4 text-center">No organizations yet.</p>
        {:else}
          {#each orgs as org}
            <button class="w-full flex items-center gap-3 px-2 py-2 rounded text-left text-sm transition-colors {selectedOrg?.id === org.id ? 'bg-nexora-accent/20 border border-nexora-accent/50' : 'hover:bg-nexora-border/50'}" onclick={() => selectOrg(org)}>
              <div class="flex-1 min-w-0">
                <div class="font-medium truncate">{org.name}</div>
                <div class="text-xs text-nexora-muted font-mono">{org.slug}</div>
              </div>
              <span class="badge-{tierBadge(org.tier)}">{org.tier}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>

    <!-- Right: Org details + members -->
    <div class="lg:col-span-2">
      {#if selectedOrg}
        <div class="card mb-4">
          <div class="flex items-start justify-between mb-4">
            <div>
              <h2 class="text-lg font-semibold">{selectedOrg.name}</h2>
              <p class="text-xs text-nexora-muted font-mono">{selectedOrg.id}</p>
            </div>
            <span class="badge-{tierBadge(selectedOrg.tier)}">{selectedOrg.tier}</span>
          </div>
          <dl class="grid grid-cols-2 gap-4 text-sm">
            <div><dt class="text-xs text-nexora-muted">Slug</dt><dd class="font-mono">{selectedOrg.slug}</dd></div>
            <div><dt class="text-xs text-nexora-muted">Max Members</dt><dd class="font-mono">{selectedOrg.max_members}</dd></div>
            <div><dt class="text-xs text-nexora-muted">Active</dt><dd>{selectedOrg.active ? '✓ Yes' : '✗ No'}</dd></div>
            <div><dt class="text-xs text-nexora-muted">Created</dt><dd>{formatDate(selectedOrg.created_at)}</dd></div>
          </dl>
        </div>

        <div class="card">
          <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Members ({members.length})</h3>
          <table class="w-full text-sm mb-4">
            <thead><tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border">
              <th class="py-2 pr-4">User ID</th>
              <th class="py-2 pr-4">Role</th>
              <th class="py-2 pr-4">Joined</th>
            </tr></thead>
            <tbody>
              {#each members as m}
                <tr class="border-b border-nexora-border/50">
                  <td class="py-2 pr-4 font-mono text-xs">{m.user_id.slice(0, 12)}…</td>
                  <td class="py-2 pr-4"><span class="badge-{m.role === 'owner' ? 'success' : 'muted'}">{m.role}</span></td>
                  <td class="py-2 pr-4 text-xs text-nexora-muted">{formatDate(m.joined_at)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <div class="flex gap-2">
            <input class="input text-sm" placeholder="User ID to invite" bind:value={newMemberId} />
            <select class="input text-sm w-32" bind:value={newMemberRole}>
              <option value="member">Member</option>
              <option value="admin">Admin</option>
              <option value="viewer">Viewer</option>
              <option value="billing">Billing</option>
            </select>
            <button class="btn-primary text-sm" onclick={handleAddMember} disabled={!newMemberId}>Add</button>
          </div>
        </div>
      {:else}
        <div class="card text-center text-nexora-muted">Select an organization to view details.</div>
      {/if}
    </div>
  </div>
</Layout>
