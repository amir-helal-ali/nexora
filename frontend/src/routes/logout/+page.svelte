<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api/gateway';
  import { session } from '$lib/stores/session';

  onMount(async () => {
    await api.logout();
    session.clear();
    await goto('/login');
  });
</script>

<div class="min-h-screen flex items-center justify-center text-nexora-muted">
  Signing out…
</div>
