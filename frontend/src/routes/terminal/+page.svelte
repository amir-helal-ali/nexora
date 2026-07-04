<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api/gateway';
  import Layout from '$lib/components/Layout.svelte';

  let ws: WebSocket | null = $state(null);
  let connected = $state(false);
  let messages = $state<Array<{ dir: 'in' | 'out'; text: string; time: string }>>([]);
  let input = $state('');
  let autoScroll = $state(true);

  const quickCommands = [
    { label: 'Ping', cmd: '{"type":"ping"}' },
    { label: 'Core Ping', cmd: '{"type":"core_ping"}' },
    { label: 'Billing Stats', cmd: '{"type":"billing_stats"}' },
    { label: 'Workflow Stats', cmd: '{"type":"workflow_stats"}' },
    { label: 'Marketplace', cmd: '{"type":"marketplace_list"}' },
    { label: 'Publish Event', cmd: '{"type":"publish_event","name":"terminal.test","payload":"hello"}' },
  ];

  function timestamp(): string {
    return new Date().toLocaleTimeString();
  }

  function addMessage(dir: 'in' | 'out', text: string) {
    messages = [...messages, { dir, text, time: timestamp() }];
    if (messages.length > 100) messages = messages.slice(-100);
  }

  function connect() {
    const token = localStorage.getItem('nexora.token');
    if (!token) return;
    const encoded = encodeURIComponent(token);
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    ws = new WebSocket(`${protocol}//${host}/api/ws?token=${encoded}`);

    ws.onopen = () => {
      connected = true;
      addMessage('in', '=== WebSocket connected ===');
    };

    ws.onmessage = (event) => {
      addMessage('in', event.data);
    };

    ws.onclose = () => {
      connected = false;
      addMessage('in', '=== WebSocket disconnected ===');
    };

    ws.onerror = () => {
      addMessage('in', '=== WebSocket error ===');
    };
  }

  function disconnect() {
    ws?.close();
    ws = null;
    connected = false;
  }

  function send() {
    if (!ws || !input.trim()) return;
    ws.send(input);
    addMessage('out', input);
    input = '';
  }

  function sendQuick(cmd: string) {
    if (!ws) return;
    ws.send(cmd);
    addMessage('out', cmd);
  }

  function clearMessages() {
    messages = [];
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  $effect(() => {
    messages;
    if (autoScroll) {
      setTimeout(() => {
        const el = document.getElementById('ws-terminal');
        if (el) el.scrollTop = el.scrollHeight;
      }, 0);
    }
  });

  onMount(() => {
    connect();
  });

  onDestroy(() => {
    ws?.close();
  });
</script>

<Layout>
  <div class="mb-6 flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold mb-1">WebSocket Terminal</h1>
      <p class="text-sm text-nexora-muted">
        Bidirectional real-time communication · {messages.length} messages
      </p>
    </div>
    <div class="flex items-center gap-3">
      {#if connected}
        <span class="flex items-center gap-2 text-xs text-emerald-400">
          <span class="relative flex h-2 w-2">
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
          Connected
        </span>
        <button class="btn-ghost text-red-400" onclick={disconnect}>Disconnect</button>
      {:else}
        <span class="text-xs text-nexora-muted">Disconnected</span>
        <button class="btn-primary" onclick={connect}>Connect</button>
      {/if}
      <button class="btn-ghost" onclick={clearMessages}>Clear</button>
    </div>
  </div>

  <!-- Quick commands -->
  <div class="card mb-4">
    <h2 class="text-xs font-semibold uppercase tracking-wider text-nexora-muted mb-3">Quick Commands</h2>
    <div class="flex flex-wrap gap-2">
      {#each quickCommands as q}
        <button
          class="px-3 py-1.5 rounded-md text-xs font-mono bg-nexora-bg border border-nexora-border hover:border-nexora-accent hover:text-nexora-accent transition-colors"
          onclick={() => sendQuick(q.cmd)}
          disabled={!connected}
        >
          {q.label}
        </button>
      {/each}
    </div>
  </div>

  <!-- Terminal output -->
  <div class="card !p-0 overflow-hidden mb-4">
    <div
      id="ws-terminal"
      class="h-[400px] overflow-y-auto p-4 font-mono text-xs space-y-1 bg-nexora-bg"
    >
      {#if messages.length === 0}
        <p class="text-nexora-muted">No messages yet. Use quick commands or type below.</p>
      {/if}
      {#each messages as msg}
        <div class="flex gap-2 {msg.dir === 'out' ? 'text-nexora-accent' : 'text-nexora-text'}">
          <span class="text-nexora-muted shrink-0">{msg.time}</span>
          <span class="shrink-0">{msg.dir === 'out' ? '→' : '←'}</span>
          <span class="break-all">{msg.text}</span>
        </div>
      {/each}
    </div>
  </div>

  <!-- Input -->
  <div class="card">
    <div class="flex gap-2">
      <input
        class="input font-mono"
        placeholder="Type a JSON message and press Enter"
        bind:value={input}
        onkeydown={handleKeydown}
        disabled={!connected}
      />
      <button class="btn-primary" onclick={send} disabled={!connected || !input.trim()}>
        Send
      </button>
    </div>
    <p class="mt-2 text-xs text-nexora-muted">
      Press Enter to send · Shift+Enter for newline · Live events from the EventBus are pushed automatically
    </p>
  </div>
</Layout>
