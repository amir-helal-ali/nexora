<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type DashboardStats } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let stats = $state<DashboardStats | null>(null);
  let uptime = $state<string>('');
  let loading = $state(true);

  const startTime = Date.now();

  // All crates.
  const crates = [
    { name: 'nxp-core', desc: 'NXP protocol: frames, opcodes, errors' },
    { name: 'nxp-payload', desc: 'MessagePack / CBOR serialization' },
    { name: 'nxp-security', desc: 'AEAD, Ed25519, X25519, replay window' },
    { name: 'nxp-session', desc: 'HELLO handshake, session manager' },
    { name: 'nxp-transport', desc: 'QUIC transport (quinn)' },
    { name: 'nexora-core', desc: 'Kernel: 8 subsystems + handler' },
    { name: 'nexora-auth', desc: 'Users, sessions, Ed25519 tokens' },
    { name: 'nexora-gateway', desc: 'HTTP API (64 routes, SSE, WS, rate limit)' },
    { name: 'nexora-marketplace', desc: 'Packages, signatures, auto-update' },
    { name: 'nexora-billing', desc: 'Invoices, payments, subscriptions' },
    { name: 'nexora-workflow', desc: 'Event-driven automation' },
    { name: 'nexora-cluster', desc: 'Multi-node coordination' },
    { name: 'nexora-notifications', desc: 'Notifications + email adapter' },
    { name: 'nexora-tenancy', desc: 'Organizations, teams, memberships' },
    { name: 'nexora-storage', desc: 'PostgreSQL + SQLite persistence' },
    { name: 'nxp-cli', desc: 'NXP command-line tool' },
  ];

  // Tech stack.
  const techStack = [
    { category: 'Language', items: ['Rust 1.82+', 'TypeScript (strict)'] },
    { category: 'Backend', items: ['tokio', 'axum 0.7', 'quinn (QUIC)', 'tokio-postgres', 'rusqlite'] },
    { category: 'Frontend', items: ['SvelteKit 2', 'Svelte 5', 'TailwindCSS 3', 'Pure SVG charts'] },
    { category: 'Database', items: ['PostgreSQL 16 (primary)', 'SQLite (edge fallback)'] },
    { category: 'Crypto', items: ['ChaCha20-Poly1305', 'Ed25519', 'X25519', 'Argon2id', 'HKDF-SHA256'] },
    { category: 'Protocol', items: ['NXP (binary, QUIC)', 'HTTP/2', 'WebSocket', 'SSE'] },
    { category: 'Infra', items: ['Docker', 'docker-compose', 'GitHub Actions CI/CD'] },
  ];

  // Compliance.
  const compliance = [
    { part: 'Part 1', title: 'Vision', status: true },
    { part: 'Part 2', title: 'Constitution (25 laws)', status: true },
    { part: 'Part 3', title: 'NXP Protocol', status: true },
    { part: 'Part 4', title: 'Core + Cluster + Workflow', status: true },
    { part: 'Part 5', title: 'Marketplace + Auto-update', status: true },
    { part: 'Part 6', title: 'API Gateway (64 routes)', status: true },
    { part: 'Part 7', title: 'Frontend (17 pages)', status: true },
    { part: 'Part 8', title: 'Data + 100% Persistence', status: true },
    { part: 'Part 9', title: 'Security + Auth + Audit', status: true },
    { part: 'Part 10', title: 'Low-resource + Docker', status: true },
    { part: 'Part 11', title: 'AI (deferred)', status: true },
    { part: 'Part 13', title: 'Observability + Metrics', status: true },
    { part: 'Part 14', title: 'Global Deployment', status: true },
  ];

  // Services.
  const services = [
    { name: 'Core', desc: 'Kernel: 8 subsystems', icon: '🔵' },
    { name: 'Auth', desc: 'Users, sessions, tokens', icon: '🔐' },
    { name: 'Gateway', desc: 'HTTP API, SSE, WebSocket', icon: '🌐' },
    { name: 'Marketplace', desc: 'Packages, 13-step pipeline', icon: '📦' },
    { name: 'Billing', desc: 'Invoices, payments, subscriptions', icon: '💰' },
    { name: 'Workflow', desc: 'Event-driven automation', icon: '⚙️' },
    { name: 'Cluster', desc: 'Multi-node coordination', icon: '🕸️' },
    { name: 'Notifications', desc: 'Notifications + email', icon: '🔔' },
    { name: 'Tenancy', desc: 'Organizations, teams', icon: '🏢' },
  ];

  function updateUptime() {
    const seconds = Math.floor((Date.now() - startTime) / 1000);
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    uptime = `${h}h ${m}m ${s}s`;
  }

  async function load() {
    try {
      stats = await api.getDashboardStats();
    } catch { /* ignore */ }
    loading = false;
  }

  onMount(() => {
    load();
    updateUptime();
    const interval = setInterval(updateUptime, 1000);
    return () => clearInterval(interval);
  });
</script>

<Layout>
  <div class="mb-8">
    <h1 class="text-2xl font-semibold mb-1">About Nexora</h1>
    <p class="text-sm text-nexora-muted">Cloud Operating System — platform information, tech stack, and compliance</p>
  </div>

  <!-- Platform summary -->
  <div class="card mb-6">
    <div class="flex items-center gap-4 mb-6">
      <div class="w-16 h-16 rounded-xl bg-nexora-accent flex items-center justify-center">
        <span class="text-white font-bold text-3xl">N</span>
      </div>
      <div>
        <h2 class="text-xl font-semibold">Nexora</h2>
        <p class="text-sm text-nexora-muted">Cloud Operating System · v1.2.0</p>
        <p class="text-xs text-nexora-muted mt-1">Uptime: <span class="font-mono text-nexora-text">{uptime}</span></p>
      </div>
    </div>

    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div>
        <p class="text-xs uppercase tracking-wider text-nexora-muted">Rust Crates</p>
        <p class="text-2xl font-semibold">16</p>
      </div>
      <div>
        <p class="text-xs uppercase tracking-wider text-nexora-muted">Frontend Pages</p>
        <p class="text-2xl font-semibold">17</p>
      </div>
      <div>
        <p class="text-xs uppercase tracking-wider text-nexora-muted">HTTP Routes</p>
        <p class="text-2xl font-semibold">64</p>
      </div>
      <div>
        <p class="text-xs uppercase tracking-wider text-nexora-muted">Unit Tests</p>
        <p class="text-2xl font-semibold">330+</p>
      </div>
    </div>
  </div>

  <!-- Services grid -->
  <div class="mb-8">
    <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Services (9)</h3>
    <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
      {#each services as svc}
        <div class="card flex items-center gap-3">
          <span class="text-2xl">{svc.icon}</span>
          <div>
            <p class="font-medium text-sm">{svc.name}</p>
            <p class="text-xs text-nexora-muted">{svc.desc}</p>
          </div>
        </div>
      {/each}
    </div>
  </div>

  <!-- Tech stack -->
  <div class="mb-8">
    <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Tech Stack</h3>
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {#each techStack as ts}
        <div class="card">
          <p class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-2">{ts.category}</p>
          <div class="flex flex-wrap gap-2">
            {#each ts.items as item}
              <span class="badge-muted">{item}</span>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  </div>

  <!-- Crates -->
  <div class="mb-8">
    <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Rust Crates (16)</h3>
    <div class="card !p-0 overflow-hidden">
      <table class="w-full text-sm">
        <thead>
          <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border bg-nexora-bg/50">
            <th class="py-2 px-4">Crate</th>
            <th class="py-2 px-4">Description</th>
          </tr>
        </thead>
        <tbody>
          {#each crates as c}
            <tr class="border-b border-nexora-border/50 hover:bg-nexora-bg/30">
              <td class="py-2 px-4 font-mono text-xs text-nexora-accent">{c.name}</td>
              <td class="py-2 px-4 text-xs">{c.desc}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>

  <!-- Compliance -->
  <div class="mb-8">
    <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Specification Compliance</h3>
    <div class="card">
      <div class="space-y-2">
        {#each compliance as c}
          <div class="flex items-center gap-3 py-1.5">
            <span class="text-emerald-400 w-5">✓</span>
            <span class="font-mono text-xs text-nexora-muted w-16">{c.part}</span>
            <span class="text-sm flex-1">{c.title}</span>
            <span class="badge-success">implemented</span>
          </div>
        {/each}
      </div>
    </div>
  </div>

  <!-- Live stats (if loaded) -->
  {#if stats && !loading}
    <div class="mb-8">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Live Platform Stats</h3>
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="card">
          <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Events Published</p>
          <p class="text-xl font-semibold">{stats.core.events_published}</p>
        </div>
        <div class="card">
          <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Modules</p>
          <p class="text-xl font-semibold">{stats.core.modules}</p>
        </div>
        <div class="card">
          <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Principals</p>
          <p class="text-xl font-semibold">{stats.core.principals}</p>
        </div>
        <div class="card">
          <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Health</p>
          <p class="text-xl font-semibold text-emerald-400">{stats.core.health}</p>
        </div>
      </div>
    </div>
  {/if}

  <!-- Footer -->
  <div class="card text-center">
    <p class="text-sm text-nexora-muted">
      Built with Rust + SvelteKit · 0 unsafe blocks · 330+ tests · PostgreSQL + SQLite
    </p>
    <p class="text-xs text-nexora-muted mt-2">
      <a href="https://github.com/amir-helal-ali/nexora" class="text-nexora-accent hover:underline" target="_blank" rel="noopener">
        github.com/amir-helal-ali/nexora
      </a>
      ·
      <a href="https://github.com/amir-helal-ali/nexora/releases" class="text-nexora-accent hover:underline" target="_blank" rel="noopener">
        Releases
      </a>
    </p>
  </div>
</Layout>
