<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api/gateway';
  import { session } from '$lib/stores/session';

  let username = $state('');
  let password = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    if (session.authenticated) {
      goto('/');
    }
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!username || !password) return;
    loading = true;
    error = null;
    try {
      const resp = await api.login(username, password);
      session.setAuthenticated(resp.username);
      await goto('/');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Login failed';
    } finally {
      loading = false;
    }
  }
</script>

<div class="min-h-screen flex items-center justify-center px-4">
  <div class="w-full max-w-sm">
    <div class="text-center mb-8">
      <div class="inline-flex items-center justify-center w-12 h-12 rounded-lg bg-nexora-accent mb-3">
        <span class="text-white font-bold text-xl">N</span>
      </div>
      <h1 class="text-xl font-semibold">Nexora</h1>
      <p class="text-sm text-nexora-muted mt-1">Cloud Operating System</p>
    </div>

    <form class="card" onsubmit={handleSubmit}>
      <h2 class="text-lg font-medium mb-4">Sign in</h2>

      <span class="block mb-3">
        <span class="block text-xs text-nexora-muted mb-1">Username</span>
        <input
          class="input"
          type="text"
          bind:value={username}
          placeholder="admin"
          autocomplete="username"
          disabled={loading}
        />
      </span>

      <span class="block mb-4">
        <span class="block text-xs text-nexora-muted mb-1">Password</span>
        <input
          class="input"
          type="password"
          bind:value={password}
          placeholder="••••••••"
          autocomplete="current-password"
          disabled={loading}
        />
      </span>

      {#if error}
        <div class="mb-4 p-3 rounded-md bg-red-500/10 border border-red-500/20 text-sm text-red-400">
          {error}
        </div>
      {/if}

      <button type="submit" class="btn-primary w-full" disabled={loading || !username || !password}>
        {#if loading}
          <svg class="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
          Signing in…
        {:else}
          Sign in
        {/if}
      </button>

      <p class="mt-4 text-xs text-nexora-muted text-center">
        Demo: <code class="text-nexora-text">admin</code> /
        <code class="text-nexora-text">admin123</code>
      </p>
    </form>

    <p class="mt-6 text-center text-xs text-nexora-muted">
      Bearer token + Ed25519 · Argon2id passwords · HTTP ↔ NXP gateway
    </p>
  </div>
</div>
