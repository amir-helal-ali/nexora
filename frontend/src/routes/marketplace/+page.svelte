<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type PackageInfo, type InstallReport } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let packages = $state<PackageInfo[]>([]);
  let installed = $state<PackageInfo[]>([]);
  let searchQuery = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let installReport = $state<InstallReport | null>(null);
  let installLoading = $state(false);

  async function load() {
    loading = true;
    error = null;
    try {
      const [all, inst] = await Promise.all([
        searchQuery ? api.searchPackages(searchQuery) : api.listPackages(),
        api.listInstalled(),
      ]);
      packages = all.packages;
      installed = inst.packages;
      // Mark installed packages
      const installedIds = new Set(installed.map((p) => p.manifest.id));
      packages = packages.map((p) => ({
        ...p,
        installed: installedIds.has(p.manifest.id),
      }));
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  async function handleInstall(pkg: PackageInfo) {
    installLoading = true;
    installReport = null;
    try {
      const resp = await api.installPackage(pkg.manifest.id, pkg.manifest.version);
      installReport = resp.report;
      await load();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Install failed';
    } finally {
      installLoading = false;
    }
  }

  async function handleUninstall(pkg: PackageInfo) {
    try {
      await api.uninstallPackage(pkg.manifest.id);
      await load();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Uninstall failed';
    }
  }

  function billingDisplay(billing: PackageInfo['manifest']['billing']): string {
    switch (billing.kind) {
      case 'free': return 'Free';
      case 'one_time': return 'One-time';
      case 'subscription': return 'Subscription';
      case 'usage_based': return 'Usage-based';
      case 'enterprise': return 'Enterprise';
      default: return billing.kind;
    }
  }

  function typeBadge(type: string): string {
    const map: Record<string, string> = {
      module: 'success',
      plugin: 'muted',
      ai_agent: 'warning',
      template: 'muted',
      service: 'success',
      automation: 'muted',
    };
    return map[type] || 'muted';
  }

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Marketplace</h1>
      <p class="text-sm text-nexora-muted">
        {packages.length} packages · {installed.length} installed
      </p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>
      {loading ? 'Loading…' : '↻ Refresh'}
    </button>
  </div>

  <!-- Search -->
  <div class="card mb-6">
    <form
      class="flex gap-2"
      onsubmit={(e) => {
        e.preventDefault();
        load();
      }}
    >
      <input
        class="input"
        placeholder="search packages (name, description, tags)…"
        bind:value={searchQuery}
      />
      <button type="submit" class="btn-primary">Search</button>
    </form>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">
      {error}
    </div>
  {/if}

  <!-- Install report -->
  {#if installReport}
    <div class="card mb-6">
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted">
          Install Report — {installReport.package_id}@{installReport.version}
        </h2>
        <button class="btn-ghost text-xs" onclick={() => (installReport = null)}>✕</button>
      </div>
      <div class="space-y-1 font-mono text-xs">
        {#each installReport.steps as s}
          <div class="flex items-center gap-3 p-2 rounded bg-nexora-bg border border-nexora-border">
            <span class={s.passed ? 'text-emerald-400' : 'text-red-400'}>
              {s.passed ? '✓' : '✗'}
            </span>
            <span class="text-nexora-muted w-6">#{s.step}</span>
            <span class="text-nexora-text w-40">{s.name}</span>
            <span class="text-nexora-muted flex-1">{s.message || ''}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Packages grid -->
  {#if packages.length === 0 && !loading}
    <div class="card text-center text-nexora-muted">
      No packages found. {searchQuery ? 'Try a different search.' : 'Publish one via POST /api/marketplace/packages'}
    </div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {#each packages as pkg}
        <div class="card flex flex-col">
          <div class="flex items-start justify-between mb-3">
            <div class="flex-1 min-w-0">
              <h3 class="font-semibold truncate">{pkg.manifest.name}</h3>
              <p class="text-xs text-nexora-muted font-mono truncate">{pkg.manifest.id}</p>
            </div>
            <span class="badge-{typeBadge(pkg.manifest.package_type)} ml-2">
              {pkg.manifest.package_type}
            </span>
          </div>

          <p class="text-sm text-nexora-text mb-3 line-clamp-2">{pkg.manifest.description}</p>

          <dl class="space-y-2 text-xs mb-4">
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Version</dt>
              <dd class="font-mono">{pkg.manifest.version}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Owner</dt>
              <dd>{pkg.manifest.owner_name}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Billing</dt>
              <dd>{billingDisplay(pkg.manifest.billing)}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-nexora-muted">Installs</dt>
              <dd class="font-mono">{pkg.install_count} ({pkg.active_install_count} active)</dd>
            </div>
          </dl>

          <!-- Tags -->
          {#if pkg.manifest.tags.length > 0}
            <div class="flex flex-wrap gap-1 mb-4">
              {#each pkg.manifest.tags as tag}
                <span class="badge-muted">{tag}</span>
              {/each}
            </div>
          {/if}

          <!-- Integrity hash -->
          <div class="text-xs text-nexora-muted font-mono mb-4 truncate" title={pkg.integrity_hash}>
            sha256: {pkg.integrity_hash.slice(0, 16)}…
          </div>

          <!-- Actions -->
          <div class="mt-auto pt-3 border-t border-nexora-border">
            {#if pkg.installed}
              <button
                class="btn-ghost w-full text-red-400 hover:text-red-300"
                onclick={() => handleUninstall(pkg)}
              >
                Uninstall
              </button>
            {:else}
              <button
                class="btn-primary w-full"
                onclick={() => handleInstall(pkg)}
                disabled={installLoading}
              >
                {installLoading ? 'Installing…' : 'Install'}
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</Layout>
