<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  interface Invoice {
    id: string;
    customer_id: string;
    customer_name: string;
    items: Array<{
      description: string;
      package_id: string | null;
      quantity: number;
      unit_price_minor: number;
      total_minor: number;
      currency: string;
    }>;
    total_minor: number;
    currency: string;
    status: string;
    created_at: number;
    due_at: number;
    paid_at: number | null;
    subscription_id: string | null;
    payment_ids: string[];
  }

  interface Payment {
    id: string;
    invoice_id: string;
    customer_id: string;
    amount_minor: number;
    currency: string;
    status: string;
    method: string;
    created_at: number;
    processed_at: number | null;
    failure_reason: string | null;
  }

  interface Subscription {
    id: string;
    customer_id: string;
    package_id: string;
    price_minor: number;
    currency: string;
    period_seconds: number;
    status: string;
    started_at: number;
    current_period_end: number;
    cancelled_at: number | null;
  }

  interface BillingStats {
    invoice_count: number;
    payment_count: number;
    subscription_count: number;
    revenue_minor: number;
    outstanding_minor: number;
    currency: string;
  }

  let invoices = $state<Invoice[]>([]);
  let payments = $state<Payment[]>([]);
  let subscriptions = $state<Subscription[]>([]);
  let stats = $state<BillingStats | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      const [invResp, payResp, subResp, statsResp] = await Promise.all([
        api.request('/api/billing/invoices'),
        api.request('/api/billing/payments'),
        api.request('/api/billing/subscriptions'),
        api.request('/api/billing/stats'),
      ]);
      invoices = invResp.invoices || [];
      payments = payResp.payments || [];
      subscriptions = subResp.subscriptions || [];
      stats = statsResp.stats;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  function formatAmount(minor: number, currency: string): string {
    return `${(minor / 100).toFixed(2)} ${currency}`;
  }

  function statusBadge(status: string): string {
    const map: Record<string, string> = {
      paid: 'success',
      succeeded: 'success',
      active: 'success',
      open: 'warning',
      pending: 'warning',
      past_due: 'error',
      failed: 'error',
      void: 'muted',
      cancelled: 'muted',
      ended: 'muted',
    };
    return map[status] || 'muted';
  }

  function formatDate(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleDateString();
  }

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Billing</h1>
      <p class="text-sm text-nexora-muted">
        {invoices.length} invoices · {payments.length} payments · {subscriptions.length} subscriptions
      </p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>
      {loading ? 'Loading…' : '↻ Refresh'}
    </button>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">
      {error}
    </div>
  {/if}

  <!-- Stats grid -->
  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Revenue</p>
        <p class="text-2xl font-semibold text-emerald-400">
          {formatAmount(stats.revenue_minor, stats.currency)}
        </p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Outstanding</p>
        <p class="text-2xl font-semibold text-amber-400">
          {formatAmount(stats.outstanding_minor, stats.currency)}
        </p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Invoices</p>
        <p class="text-2xl font-semibold">{stats.invoice_count}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Subscriptions</p>
        <p class="text-2xl font-semibold">{stats.subscription_count}</p>
      </div>
    </div>
  {/if}

  <!-- Invoices table -->
  <div class="card mb-6">
    <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-3">Invoices</h2>
    {#if invoices.length === 0}
      <p class="text-sm text-nexora-muted">No invoices yet.</p>
    {:else}
      <table class="w-full text-sm">
        <thead>
          <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border">
            <th class="py-2 pr-4">ID</th>
            <th class="py-2 pr-4">Customer</th>
            <th class="py-2 pr-4">Amount</th>
            <th class="py-2 pr-4">Status</th>
            <th class="py-2 pr-4">Created</th>
          </tr>
        </thead>
        <tbody class="font-mono text-xs">
          {#each invoices as inv}
            <tr class="border-b border-nexora-border/50 hover:bg-nexora-bg/50">
              <td class="py-2 pr-4 text-nexora-muted">{inv.id.slice(0, 8)}…</td>
              <td class="py-2 pr-4">{inv.customer_name}</td>
              <td class="py-2 pr-4">{formatAmount(inv.total_minor, inv.currency)}</td>
              <td class="py-2 pr-4"><span class="badge-{statusBadge(inv.status)}">{inv.status}</span></td>
              <td class="py-2 pr-4 text-nexora-muted">{formatDate(inv.created_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <!-- Payments table -->
  <div class="card mb-6">
    <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-3">Payments</h2>
    {#if payments.length === 0}
      <p class="text-sm text-nexora-muted">No payments yet.</p>
    {:else}
      <table class="w-full text-sm">
        <thead>
          <tr class="text-left text-xs uppercase tracking-wider text-nexora-muted border-b border-nexora-border">
            <th class="py-2 pr-4">ID</th>
            <th class="py-2 pr-4">Invoice</th>
            <th class="py-2 pr-4">Amount</th>
            <th class="py-2 pr-4">Method</th>
            <th class="py-2 pr-4">Status</th>
          </tr>
        </thead>
        <tbody class="font-mono text-xs">
          {#each payments as pay}
            <tr class="border-b border-nexora-border/50 hover:bg-nexora-bg/50">
              <td class="py-2 pr-4 text-nexora-muted">{pay.id.slice(0, 8)}…</td>
              <td class="py-2 pr-4 text-nexora-muted">{pay.invoice_id.slice(0, 8)}…</td>
              <td class="py-2 pr-4">{formatAmount(pay.amount_minor, pay.currency)}</td>
              <td class="py-2 pr-4">{pay.method}</td>
              <td class="py-2 pr-4"><span class="badge-{statusBadge(pay.status)}">{pay.status}</span></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <!-- Subscriptions -->
  <div class="card">
    <h2 class="text-sm font-semibold uppercase tracking-wider text-nexora-muted mb-3">Subscriptions</h2>
    {#if subscriptions.length === 0}
      <p class="text-sm text-nexora-muted">No subscriptions yet.</p>
    {:else}
      <div class="space-y-3">
        {#each subscriptions as sub}
          <div class="p-3 rounded bg-nexora-bg border border-nexora-border">
            <div class="flex items-center justify-between">
              <div>
                <p class="font-mono text-sm text-nexora-accent">{sub.package_id}</p>
                <p class="text-xs text-nexora-muted mt-1">
                  {formatAmount(sub.price_minor, sub.currency)} / {Math.round(sub.period_seconds / 86400)}d
                </p>
              </div>
              <span class="badge-{statusBadge(sub.status)}">{sub.status}</span>
            </div>
            <p class="text-xs text-nexora-muted mt-2">
              Current period ends: {formatDate(sub.current_period_end)}
            </p>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Layout>
