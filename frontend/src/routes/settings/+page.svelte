<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';
  import { goto } from '$app/navigation';
  import Layout from '$lib/components/Layout.svelte';

  interface UserProfile {
    id: string;
    username: string;
    email: string | null;
    roles: string[];
    active: boolean;
    created_at: number;
    last_login: number | null;
  }

  interface UserSession {
    id: string;
    user_id: string;
    created_at: number;
    last_active: number;
    active: boolean;
    client: string | null;
  }

  interface UserListItem {
    id: string;
    username: string;
    email: string | null;
    roles: string[];
    active: boolean;
    created_at: number;
    last_login: number | null;
  }

  let profile = $state<UserProfile | null>(null);
  let sessions = $state<UserSession[]>([]);
  let users = $state<UserListItem[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let activeTab = $state<'profile' | 'sessions' | 'users'>('profile');

  // Change password form.
  let currentPassword = $state('');
  let newPassword = $state('');
  let confirmPassword = $state('');
  let passwordLoading = $state(false);
  let passwordResult = $state<string | null>(null);

  // Create user form.
  let newUsername = $state('');
  let newPasswordUser = $state('');
  let newEmail = $state('');
  let newRoles = $state('viewer');
  let createUserLoading = $state(false);
  let createUserResult = $state<string | null>(null);

  // Token display.
  let token = $state<string | null>(null);
  let showToken = $state(false);

  async function loadAll() {
    loading = true;
    error = null;
    try {
      const [profileResp, sessionsResp, usersResp] = await Promise.all([
        api.request<{ ok: boolean; user: UserProfile }>('/api/users/me'),
        api.request<{ ok: boolean; sessions: UserSession[] }>('/api/users/sessions'),
        api.request<{ ok: boolean; users: UserListItem[] }>('/api/users'),
      ]);
      profile = profileResp.user;
      sessions = sessionsResp.sessions;
      users = usersResp.users;
      token = localStorage.getItem('nexora.token');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  async function handleChangePassword(e: Event) {
    e.preventDefault();
    if (newPassword !== confirmPassword) {
      passwordResult = 'Error: passwords do not match';
      return;
    }
    passwordLoading = true;
    passwordResult = null;
    try {
      const resp = await api.request<{ ok: boolean; message?: string }>('/api/users/change_password', {
        method: 'POST',
        body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
      });
      passwordResult = resp.ok ? 'Password changed successfully' : 'Failed to change password';
      currentPassword = '';
      newPassword = '';
      confirmPassword = '';
    } catch (err) {
      passwordResult = `Error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      passwordLoading = false;
    }
  }

  async function handleCreateUser(e: Event) {
    e.preventDefault();
    createUserLoading = true;
    createUserResult = null;
    try {
      const resp = await api.request<{ ok: boolean; user?: UserProfile }>('/api/users', {
        method: 'POST',
        body: JSON.stringify({
          username: newUsername,
          password: newPasswordUser,
          email: newEmail || null,
          roles: newRoles.split(',').map((r) => r.trim()).filter(Boolean),
        }),
      });
      if (resp.ok) {
        createUserResult = `User "${newUsername}" created successfully`;
        newUsername = '';
        newPasswordUser = '';
        newEmail = '';
        newRoles = 'viewer';
        await loadAll();
      } else {
        createUserResult = 'Failed to create user';
      }
    } catch (err) {
      createUserResult = `Error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      createUserLoading = false;
    }
  }

  async function handleDeleteUser(id: string, username: string) {
    if (!confirm(`Delete user "${username}"? This cannot be undone.`)) return;
    try {
      await api.request(`/api/users/${encodeURIComponent(id)}`, { method: 'DELETE' });
      await loadAll();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Delete failed';
    }
  }

  function formatDate(ns: number | null): string {
    if (!ns) return '—';
    return new Date(ns / 1_000_000).toLocaleString();
  }

  function copyToken() {
    if (token) {
      navigator.clipboard.writeText(token);
    }
  }

  onMount(() => {
    loadAll();
  });
</script>

<Layout>
  <div class="mb-8">
    <h1 class="text-2xl font-semibold mb-1">Settings</h1>
    <p class="text-sm text-nexora-muted">Manage your profile, sessions, and users</p>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">{error}</div>
  {/if}

  <!-- Tabs -->
  <div class="flex gap-1 mb-6 border-b border-nexora-border">
    <button
      class="px-4 py-2 text-sm font-medium transition-colors {activeTab === 'profile' ? 'text-nexora-accent border-b-2 border-nexora-accent' : 'text-nexora-muted hover:text-nexora-text'}"
      onclick={() => (activeTab = 'profile')}
    >
      Profile
    </button>
    <button
      class="px-4 py-2 text-sm font-medium transition-colors {activeTab === 'sessions' ? 'text-nexora-accent border-b-2 border-nexora-accent' : 'text-nexora-muted hover:text-nexora-text'}"
      onclick={() => (activeTab = 'sessions')}
    >
      Sessions ({sessions.length})
    </button>
    <button
      class="px-4 py-2 text-sm font-medium transition-colors {activeTab === 'users' ? 'text-nexora-accent border-b-2 border-nexora-accent' : 'text-nexora-muted hover:text-nexora-text'}"
      onclick={() => (activeTab = 'users')}
    >
      Users ({users.length})
    </button>
  </div>

  {#if loading}
    <div class="card text-center text-nexora-muted">Loading…</div>
  {:else if activeTab === 'profile' && profile}
    <!-- Profile tab -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <!-- Profile info -->
      <div class="card">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-4">Profile</h2>
        <dl class="space-y-3">
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">Username</dt>
            <dd class="font-mono">{profile.username}</dd>
          </div>
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">User ID</dt>
            <dd class="font-mono text-xs">{profile.id}</dd>
          </div>
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">Email</dt>
            <dd>{profile.email || '—'}</dd>
          </div>
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">Roles</dt>
            <dd class="flex gap-1 flex-wrap justify-end">
              {#each profile.roles as role}
                <span class="badge-muted">{role}</span>
              {/each}
            </dd>
          </div>
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">Active</dt>
            <dd>{profile.active ? '✓ Yes' : '✗ No'}</dd>
          </div>
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">Created</dt>
            <dd>{formatDate(profile.created_at)}</dd>
          </div>
          <div class="flex justify-between text-sm">
            <dt class="text-nexora-muted">Last Login</dt>
            <dd>{formatDate(profile.last_login)}</dd>
          </div>
        </dl>
      </div>

      <!-- Change password -->
      <div class="card">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-4">Change Password</h2>
        <form onsubmit={handleChangePassword} class="space-y-3">
          <input type="password" class="input" placeholder="Current password" bind:value={currentPassword} autocomplete="current-password" disabled={passwordLoading} />
          <input type="password" class="input" placeholder="New password" bind:value={newPassword} autocomplete="new-password" disabled={passwordLoading} />
          <input type="password" class="input" placeholder="Confirm new password" bind:value={confirmPassword} autocomplete="new-password" disabled={passwordLoading} />
          <button type="submit" class="btn-primary w-full" disabled={passwordLoading || !currentPassword || !newPassword || !confirmPassword}>
            {passwordLoading ? 'Changing…' : 'Change Password'}
          </button>
        </form>
        {#if passwordResult}
          <div class="mt-3 p-2 rounded-md bg-nexora-bg border border-nexora-border text-xs font-mono {passwordResult.startsWith('Error') ? 'text-red-400' : 'text-emerald-400'}">
            {passwordResult}
          </div>
        {/if}
      </div>

      <!-- API token -->
      <div class="card lg:col-span-2">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-4">API Token</h2>
        <p class="text-xs text-nexora-muted mb-3">
          Your Ed25519-signed session token. Use it in the <code class="text-nexora-text">Authorization: Bearer</code> header for API calls.
        </p>
        <div class="flex gap-2">
          <input
            type={showToken ? 'text' : 'password'}
            class="input font-mono text-xs"
            value={token || ''}
            readonly
          />
          <button class="btn-ghost" onclick={() => (showToken = !showToken)}>
            {showToken ? 'Hide' : 'Show'}
          </button>
          <button class="btn-ghost" onclick={copyToken}>Copy</button>
        </div>
      </div>
    </div>

  {:else if activeTab === 'sessions'}
    <!-- Sessions tab -->
    <div class="card">
      <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-4">Active Sessions</h2>
      {#if sessions.length === 0}
        <p class="text-sm text-nexora-muted">No active sessions.</p>
      {:else}
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border">
              <th class="py-2 pr-4">Session ID</th>
              <th class="py-2 pr-4">Client</th>
              <th class="py-2 pr-4">Created</th>
              <th class="py-2 pr-4">Last Active</th>
              <th class="py-2 pr-4">Status</th>
            </tr>
          </thead>
          <tbody class="font-mono text-xs">
            {#each sessions as s}
              <tr class="border-b border-nexora-border/50">
                <td class="py-2 pr-4 text-nexora-muted">{s.id.slice(0, 8)}…</td>
                <td class="py-2 pr-4">{s.client || '—'}</td>
                <td class="py-2 pr-4">{formatDate(s.created_at)}</td>
                <td class="py-2 pr-4">{formatDate(s.last_active)}</td>
                <td class="py-2 pr-4">
                  {#if s.active}
                    <span class="badge-success">active</span>
                  {:else}
                    <span class="badge-muted">inactive</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

  {:else if activeTab === 'users'}
    <!-- Users tab -->
    <div class="space-y-6">
      <!-- Create user -->
      <div class="card">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-4">Create New User</h2>
        <form onsubmit={handleCreateUser} class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <input class="input" placeholder="Username" bind:value={newUsername} disabled={createUserLoading} />
          <input type="password" class="input" placeholder="Password" bind:value={newPasswordUser} disabled={createUserLoading} />
          <input class="input" placeholder="Email (optional)" bind:value={newEmail} disabled={createUserLoading} />
          <input class="input" placeholder="Roles (comma-separated, e.g. admin,viewer)" bind:value={newRoles} disabled={createUserLoading} />
          <button type="submit" class="btn-primary md:col-span-2" disabled={createUserLoading || !newUsername || !newPasswordUser}>
            {createUserLoading ? 'Creating…' : 'Create User'}
          </button>
        </form>
        {#if createUserResult}
          <div class="mt-3 p-2 rounded-md bg-nexora-bg border border-nexora-border text-xs font-mono {createUserResult.startsWith('Error') ? 'text-red-400' : 'text-emerald-400'}">
            {createUserResult}
          </div>
        {/if}
      </div>

      <!-- User list -->
      <div class="card">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-4">All Users ({users.length})</h2>
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border">
              <th class="py-2 pr-4">Username</th>
              <th class="py-2 pr-4">Email</th>
              <th class="py-2 pr-4">Roles</th>
              <th class="py-2 pr-4">Last Login</th>
              <th class="py-2 pr-4">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each users as u}
              <tr class="border-b border-nexora-border/50 hover:bg-nexora-bg/50">
                <td class="py-2 pr-4 font-medium">{u.username}</td>
                <td class="py-2 pr-4 text-nexora-muted">{u.email || '—'}</td>
                <td class="py-2 pr-4">
                  <div class="flex gap-1 flex-wrap">
                    {#each u.roles as role}
                      <span class="badge-muted">{role}</span>
                    {/each}
                  </div>
                </td>
                <td class="py-2 pr-4 text-nexora-muted text-xs">{formatDate(u.last_login)}</td>
                <td class="py-2 pr-4">
                  {#if u.id !== profile?.id}
                    <button
                      class="text-xs text-red-400 hover:text-red-300"
                      onclick={() => handleDeleteUser(u.id, u.username)}
                    >
                      Delete
                    </button>
                  {:else}
                    <span class="text-xs text-nexora-muted">(you)</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}
</Layout>
