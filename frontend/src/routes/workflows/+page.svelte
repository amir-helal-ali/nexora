<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  interface WorkflowStep {
    name: string;
    action: {
      kind: string;
      name?: string;
      payload?: string;
      message?: string;
      seconds?: number;
      condition?: string;
    };
  }

  interface Workflow {
    id: string;
    name: string;
    description: string;
    trigger: {
      kind: string;
      event_name?: string;
      interval_seconds?: number;
    };
    steps: WorkflowStep[];
    enabled: boolean;
    created_at: number;
    execution_count: number;
  }

  interface StepResult {
    step_name: string;
    action: string;
    success: boolean;
    message: string | null;
  }

  interface WorkflowExecution {
    id: string;
    workflow_id: string;
    trigger_event: string | null;
    trigger_payload: string | null;
    status: 'running' | 'succeeded' | 'failed' | 'stopped';
    step_results: StepResult[];
    started_at: number;
    finished_at: number | null;
    error: string | null;
  }

  interface WorkflowStats {
    workflow_count: number;
    execution_count: number;
    succeeded: number;
    failed: number;
    stopped: number;
  }

  let workflows = $state<Workflow[]>([]);
  let executions = $state<WorkflowExecution[]>([]);
  let stats = $state<WorkflowStats | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let activeTab = $state<'workflows' | 'executions'>('workflows');

  // Create form
  let wfId = $state('');
  let wfName = $state('');
  let wfTriggerKind = $state('manual');
  let wfEventName = $state('');
  let wfSteps = $state<Array<{ name: string; actionKind: string; actionValue: string }>>([
    { name: 'step1', actionKind: 'log', actionValue: 'Hello from workflow' },
  ]);
  let createLoading = $state(false);
  let createResult = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      const [wfResp, execResp, statsResp] = await Promise.all([
        api.request<{ ok: boolean; workflows: Workflow[] }>('/api/workflows'),
        api.request<{ ok: boolean; executions: WorkflowExecution[] }>('/api/workflows/executions'),
        api.request<{ ok: boolean; stats: WorkflowStats }>('/api/workflows/stats'),
      ]);
      workflows = wfResp.workflows || [];
      executions = execResp.executions || [];
      stats = statsResp.stats;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Load failed';
    } finally {
      loading = false;
    }
  }

  async function handleCreate(e: Event) {
    e.preventDefault();
    createLoading = true;
    createResult = null;

    const trigger: Record<string, unknown> = { kind: wfTriggerKind };
    if (wfTriggerKind === 'event') {
      trigger.event_name = wfEventName;
    } else if (wfTriggerKind === 'schedule') {
      trigger.interval_seconds = 60;
    }

    const steps: WorkflowStep[] = wfSteps.map((s) => {
      const action: Record<string, unknown> = { kind: s.actionKind };
      if (s.actionKind === 'publish_event') {
        action.name = s.actionValue.split(',')[0] || 'workflow.event';
        action.payload = s.actionValue.split(',')[1] || '';
      } else if (s.actionKind === 'log') {
        action.message = s.actionValue;
      } else if (s.actionKind === 'wait') {
        action.seconds = parseInt(s.actionValue) || 1;
      } else if (s.actionKind === 'condition') {
        action.condition = s.actionValue;
      }
      return { name: s.name, action: action as WorkflowStep['action'] };
    });

    const workflow = {
      id: wfId || `wf-${Date.now().toString(36)}`,
      name: wfName,
      description: '',
      trigger,
      steps,
      enabled: true,
      created_at: 0,
      execution_count: 0,
    };

    try {
      const resp = await api.request<{ ok: boolean }>('/api/workflows', {
        method: 'POST',
        body: JSON.stringify(workflow),
      });
      if (resp.ok) {
        createResult = `Workflow "${wfName}" created`;
        wfId = '';
        wfName = '';
        wfEventName = '';
        wfSteps = [{ name: 'step1', actionKind: 'log', actionValue: 'Hello from workflow' }];
        await load();
      } else {
        createResult = 'Failed to create workflow';
      }
    } catch (err) {
      createResult = `Error: ${err instanceof Error ? err.message : 'failed'}`;
    } finally {
      createLoading = false;
    }
  }

  async function handleTrigger(id: string) {
    try {
      await api.request(`/api/workflows/${encodeURIComponent(id)}/trigger`, {
        method: 'POST',
        body: JSON.stringify({}),
      });
      await load();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Trigger failed';
    }
  }

  function addStep() {
    wfSteps = [...wfSteps, { name: `step${wfSteps.length + 1}`, actionKind: 'log', actionValue: '' }];
  }

  function removeStep(idx: number) {
    wfSteps = wfSteps.filter((_, i) => i !== idx);
  }

  function statusBadge(status: string): 'success' | 'warning' | 'error' | 'muted' {
    switch (status) {
      case 'succeeded': return 'success';
      case 'running': return 'warning';
      case 'failed': return 'error';
      default: return 'muted';
    }
  }

  function triggerDisplay(trigger: Workflow['trigger']): string {
    switch (trigger.kind) {
      case 'event': return `event: ${trigger.event_name}`;
      case 'manual': return 'manual';
      case 'schedule': return `every ${trigger.interval_seconds}s`;
      default: return trigger.kind;
    }
  }

  function actionDisplay(action: WorkflowStep['action']): string {
    switch (action.kind) {
      case 'publish_event': return `publish: ${action.name}`;
      case 'log': return `log: ${(action.message || '').slice(0, 30)}`;
      case 'wait': return `wait: ${action.seconds}s`;
      case 'condition': return `if: ${action.condition}`;
      default: return action.kind;
    }
  }

  function formatDate(ns: number): string {
    return new Date(ns / 1_000_000).toLocaleString();
  }

  onMount(() => {
    load();
  });
</script>

<Layout>
  <div class="mb-8 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">Workflows</h1>
      <p class="text-sm text-nexora-muted">
        {workflows.length} workflows · {executions.length} executions
        {#if stats}
          · {stats.succeeded} succeeded · {stats.failed} failed
        {/if}
      </p>
    </div>
    <button class="btn-ghost" onclick={load} disabled={loading}>
      {loading ? 'Loading…' : '↻ Refresh'}
    </button>
  </div>

  {#if error}
    <div class="mb-6 p-4 rounded-md bg-red-500/10 border border-red-500/20 text-red-400">{error}</div>
  {/if}

  <!-- Stats -->
  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Workflows</p>
        <p class="text-2xl font-semibold">{stats.workflow_count}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Executions</p>
        <p class="text-2xl font-semibold">{stats.execution_count}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Succeeded</p>
        <p class="text-2xl font-semibold text-emerald-400">{stats.succeeded}</p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-nexora-muted mb-2">Failed</p>
        <p class="text-2xl font-semibold text-red-400">{stats.failed}</p>
      </div>
    </div>
  {/if}

  <!-- Tabs -->
  <div class="flex gap-1 mb-6 border-b border-nexora-border">
    <button
      class="px-4 py-2 text-sm font-medium transition-colors {activeTab === 'workflows' ? 'text-nexora-accent border-b-2 border-nexora-accent' : 'text-nexora-muted hover:text-nexora-text'}"
      onclick={() => (activeTab = 'workflows')}
    >
      Workflows ({workflows.length})
    </button>
    <button
      class="px-4 py-2 text-sm font-medium transition-colors {activeTab === 'executions' ? 'text-nexora-accent border-b-2 border-nexora-accent' : 'text-nexora-muted hover:text-nexora-text'}"
      onclick={() => (activeTab = 'executions')}
    >
      Executions ({executions.length})
    </button>
  </div>

  {#if activeTab === 'workflows'}
    <!-- Create form -->
    <div class="card mb-6">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-4">Create New Workflow</h3>
      <form onsubmit={handleCreate} class="space-y-4">
        <!-- Basic info -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <input class="input" placeholder="Workflow ID (optional, auto-generated)" bind:value={wfId} disabled={createLoading} />
          <input class="input" placeholder="Workflow name" bind:value={wfName} disabled={createLoading} />
        </div>

        <!-- Trigger -->
        <div>
          <label class="block text-xs text-nexora-muted mb-1">Trigger</label>
          <div class="flex gap-2">
            <select class="input flex-shrink-0" bind:value={wfTriggerKind} disabled={createLoading}>
              <option value="manual">Manual</option>
              <option value="event">Event</option>
              <option value="schedule">Schedule</option>
            </select>
            {#if wfTriggerKind === 'event'}
              <input class="input" placeholder="Event name prefix (e.g. user.)" bind:value={wfEventName} disabled={createLoading} />
            {/if}
          </div>
        </div>

        <!-- Steps -->
        <div>
          <label class="block text-xs text-nexora-muted mb-2">Steps</label>
          <div class="space-y-2">
            {#each wfSteps as step, idx}
              <div class="flex gap-2 items-center">
                <input class="input flex-shrink-0 w-32" placeholder="step name" bind:value={step.name} disabled={createLoading} />
                <select class="input flex-shrink-0 w-40" bind:value={step.actionKind} disabled={createLoading}>
                  <option value="log">Log</option>
                  <option value="publish_event">Publish Event</option>
                  <option value="wait">Wait</option>
                  <option value="condition">Condition</option>
                </select>
                <input
                  class="input flex-1"
                  placeholder={step.actionKind === 'publish_event' ? 'event_name,payload' : step.actionKind === 'wait' ? 'seconds' : step.actionKind === 'condition' ? 'condition (true/false)' : 'message'}
                  bind:value={step.actionValue}
                  disabled={createLoading}
                />
                <button type="button" class="btn-ghost text-red-400 px-2" onclick={() => removeStep(idx)} disabled={wfSteps.length <= 1}>✕</button>
              </div>
            {/each}
          </div>
          <button type="button" class="btn-ghost mt-2 text-xs" onclick={addStep} disabled={createLoading}>+ Add Step</button>
        </div>

        <button type="submit" class="btn-primary w-full" disabled={createLoading || !wfName}>
          {createLoading ? 'Creating…' : 'Create Workflow'}
        </button>
      </form>
      {#if createResult}
        <div class="mt-3 p-2 rounded-md bg-nexora-bg border border-nexora-border text-xs font-mono {createResult.startsWith('Error') ? 'text-red-400' : 'text-emerald-400'}">
          {createResult}
        </div>
      {/if}
    </div>

    <!-- Workflow list -->
    {#if workflows.length === 0 && !loading}
      <div class="card text-center text-nexora-muted">No workflows yet. Create one above.</div>
    {:else}
      <div class="space-y-4">
        {#each workflows as wf}
          <div class="card">
            <div class="flex items-start justify-between mb-3">
              <div>
                <div class="flex items-center gap-2">
                  <h3 class="font-semibold">{wf.name}</h3>
                  {#if wf.enabled}
                    <span class="badge-success">enabled</span>
                  {:else}
                    <span class="badge-muted">disabled</span>
                  {/if}
                </div>
                <p class="text-xs text-nexora-muted font-mono mt-1">{wf.id}</p>
              </div>
              <div class="flex items-center gap-3">
                <span class="text-xs text-nexora-muted">{wf.execution_count} runs</span>
                <button class="btn-primary text-xs px-3 py-1" onclick={() => handleTrigger(wf.id)}>
                  ▶ Trigger
                </button>
              </div>
            </div>

            <!-- Trigger + steps -->
            <div class="grid grid-cols-1 md:grid-cols-3 gap-4 text-xs">
              <div>
                <span class="text-nexora-muted">Trigger:</span>
                <span class="font-mono text-nexora-accent ml-1">{triggerDisplay(wf.trigger)}</span>
              </div>
              <div class="md:col-span-2">
                <span class="text-nexora-muted">Steps:</span>
                <div class="mt-1 space-y-1">
                  {#each wf.steps as step, i}
                    <div class="flex items-center gap-2 font-mono">
                      <span class="text-nexora-muted">{i + 1}.</span>
                      <span>{step.name}</span>
                      <span class="text-nexora-muted">→</span>
                      <span class="text-nexora-accent">{actionDisplay(step.action)}</span>
                    </div>
                  {/each}
                </div>
              </div>
            </div>

            <div class="mt-3 pt-3 border-t border-nexora-border text-xs text-nexora-muted">
              Created: {formatDate(wf.created_at)}
            </div>
          </div>
        {/each}
      </div>
    {/if}

  {:else if activeTab === 'executions'}
    <!-- Executions tab -->
    {#if executions.length === 0 && !loading}
      <div class="card text-center text-nexora-muted">No executions yet. Trigger a workflow to see results.</div>
    {:else}
      <div class="space-y-3">
        {#each executions.slice().reverse() as exec}
          <div class="card">
            <div class="flex items-center justify-between mb-3">
              <div class="flex items-center gap-3">
                <span class="badge-{statusBadge(exec.status)}">{exec.status}</span>
                <span class="text-sm font-mono text-nexora-muted">{exec.id.slice(0, 12)}…</span>
                {#if exec.trigger_event}
                  <span class="text-xs text-nexora-muted">triggered by: <span class="text-nexora-accent">{exec.trigger_event}</span></span>
                {/if}
              </div>
              <span class="text-xs text-nexora-muted">{formatDate(exec.started_at)}</span>
            </div>

            <!-- Step results -->
            <div class="space-y-1">
              {#each exec.step_results as sr, i}
                <div class="flex items-center gap-3 p-2 rounded bg-nexora-bg border border-nexora-border text-xs font-mono">
                  <span class={sr.success ? 'text-emerald-400' : 'text-red-400'}>
                    {sr.success ? '✓' : '✗'}
                  </span>
                  <span class="text-nexora-muted">{i + 1}.</span>
                  <span class="text-nexora-text">{sr.step_name}</span>
                  <span class="text-nexora-muted">→</span>
                  <span class="text-nexora-accent">{sr.action}</span>
                  {#if sr.message}
                    <span class="text-nexora-muted flex-1 truncate">{sr.message}</span>
                  {/if}
                </div>
              {/each}
            </div>

            {#if exec.error}
              <div class="mt-2 p-2 rounded bg-red-500/10 border border-red-500/20 text-xs text-red-400">
                {exec.error}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</Layout>
