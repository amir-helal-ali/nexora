<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface Policy {
    id: string;
    name: string;
    description: string;
    policy_type: string;
    action: string;
    severity: string;
    enabled: boolean;
    resources: string[];
    created_at: number;
  }

  let policies: Policy[] = [];
  let loading = true;
  let error = '';
  let showCreate = false;
  let newName = '';
  let newType = 'require_mfa';
  let newAction = 'deny';
  let newDesc = '';
  let newResources = '';
  let evalResource = '';
  let evalResult: any = null;

  async function loadPolicies() {
    loading = true;
    error = '';
    try {
      const resp = await api.request<{ policies: Policy[]; total: number; enabled: number }>(
        '/api/security/policies'
      );
      policies = resp.policies || [];
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل السياسات';
    } finally {
      loading = false;
    }
  }

  async function createPolicy() {
    error = '';
    if (!newName) { error = 'الاسم مطلوب'; return; }
    try {
      const resources = newResources ? newResources.split(',').map(s => s.trim()) : [];
      await api.request('/api/security/policies', {
        method: 'POST',
        body: JSON.stringify({
          name: newName,
          policy_type: newType,
          action: newAction,
          description: newDesc,
          resources
        })
      });
      showCreate = false;
      newName = ''; newDesc = ''; newResources = '';
      await loadPolicies();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الإنشاء';
    }
  }

  async function togglePolicy(id: string, currentEnabled: boolean) {
    try {
      await api.request(`/api/security/policies/${id}/toggle`, {
        method: 'POST',
        body: JSON.stringify({ enabled: !currentEnabled })
      });
      await loadPolicies();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التبديل';
    }
  }

  async function deletePolicy(id: string) {
    if (!confirm('حذف هذه السياسة؟')) return;
    try {
      await api.request(`/api/security/policies/${id}`, { method: 'DELETE' });
      await loadPolicies();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الحذف';
    }
  }

  async function evaluate() {
    if (!evalResource) return;
    try {
      evalResult = await api.request(`/api/security/policies/evaluate?resource=${encodeURIComponent(evalResource)}`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التقييم';
    }
  }

  function actionColor(a: string): string {
    return { deny: 'text-red-400', warn: 'text-yellow-400', allow: 'text-green-400', require_step_up: 'text-blue-400' }[a] || 'text-zinc-400';
  }

  onMount(loadPolicies);
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">السياسات الأمنية</h1>
      <p class="text-sm text-zinc-400 mt-1">{policies.length} سياسة · {policies.filter(p => p.enabled).length} مفعّلة</p>
    </div>
    <div class="flex gap-2">
      <button onclick={loadPolicies} class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition">تحديث</button>
      <button onclick={() => showCreate = !showCreate} class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-sm transition">
        {showCreate ? 'إلغاء' : '+ سياسة جديدة'}
      </button>
    </div>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if showCreate}
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 space-y-3">
      <h2 class="text-sm font-semibold text-white">إنشاء سياسة جديدة</h2>
      <input bind:value={newName} placeholder="اسم السياسة" class="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm" />
      <input bind:value={newDesc} placeholder="الوصف (اختياري)" class="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm" />
      <div class="grid grid-cols-2 gap-3">
        <select bind:value={newType} class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm">
          <option value="require_mfa">مطلوب MFA</option>
          <option value="account_lockout">قفل الحساب</option>
          <option value="max_sessions">حد الجلسات</option>
          <option value="time_restriction">قيود الوقت</option>
          <option value="ip_restriction">قيود IP</option>
          <option value="rate_limit">حد المعدل</option>
          <option value="password_policy">سياسة كلمات المرور</option>
          <option value="password_expiry">انتهاء كلمة المرور</option>
          <option value="session_policy">سياسة الجلسات</option>
          <option value="custom">مخصصة</option>
        </select>
        <select bind:value={newAction} class="bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm">
          <option value="allow">سماح</option>
          <option value="deny">منع</option>
          <option value="warn">تحذير</option>
          <option value="require_step_up">مصادقة إضافية</option>
        </select>
      </div>
      <input bind:value={newResources} placeholder="الموارد (مفصولة بفواصل، مثلاً: api/billing/*)" class="w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm" />
      <button onclick={createPolicy} class="px-4 py-2 bg-green-600 hover:bg-green-500 text-white rounded-lg text-sm transition">إنشاء</button>
    </div>
  {/if}

  <!-- تقييم السياسات -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
    <h3 class="text-sm font-semibold text-white mb-2">تقييم سياسة لمورد</h3>
    <div class="flex gap-2">
      <input bind:value={evalResource} placeholder="api/billing/invoices" class="flex-1 bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm" />
      <button onclick={evaluate} class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-sm transition">تقييم</button>
    </div>
    {#if evalResult}
      <div class="mt-3 text-sm">
        الإجراء: <span class={actionColor(evalResult.action)}>{evalResult.action}</span> ·
        مسموح: <span class={evalResult.allowed ? 'text-green-400' : 'text-red-400'}>{evalResult.allowed ? 'نعم' : 'لا'}</span> ·
        السبب: <span class="text-zinc-400">{evalResult.reason}</span>
      </div>
    {/if}
  </div>

  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if policies.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد سياسات</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each policies as p (p.id)}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="text-sm font-medium text-white">{p.name}</span>
                  <span class="px-2 py-0.5 text-xs bg-zinc-700 text-zinc-300 rounded-full">{p.policy_type}</span>
                  <span class={`text-xs font-mono ${actionColor(p.action)}`}>{p.action}</span>
                  {#if p.enabled}
                    <span class="text-green-400 text-xs">● مفعّلة</span>
                  {:else}
                    <span class="text-zinc-600 text-xs">○ معطّلة</span>
                  {/if}
                </div>
                {#if p.description}
                  <p class="mt-1 text-xs text-zinc-500">{p.description}</p>
                {/if}
                {#if p.resources?.length}
                  <div class="mt-1 text-xs text-zinc-600">
                    الموارد: {p.resources.map(r => `<code>${r}</code>`).join('، ')}
                  </div>
                {/if}
              </div>
              <div class="flex gap-1 flex-shrink-0">
                <button onclick={() => togglePolicy(p.id, p.enabled)} class="px-2 py-1 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded transition">
                  {p.enabled ? 'تعطيل' : 'تفعيل'}
                </button>
                <button onclick={() => deletePolicy(p.id)} class="px-2 py-1 text-xs bg-red-900/50 hover:bg-red-800 text-red-200 rounded transition">حذف</button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
