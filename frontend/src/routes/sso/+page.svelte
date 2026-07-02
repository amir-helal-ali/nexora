<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface SsoProvider {
    id: string;
    display_name: string;
    kind: 'oidc' | 'saml';
    redirect_after_login: string;
  }

  interface SsoStats {
    providers: number;
    pending_flows: number;
    active_sessions: number;
    flows_purged: number;
    sessions_purged: number;
  }

  let providers: SsoProvider[] = [];
  let stats: SsoStats | null = null;
  let loading = true;
  let error = '';

  async function loadData() {
    loading = true;
    error = '';
    try {
      const [providersResp, statsResp] = await Promise.all([
        api.request<{ providers: SsoProvider[]; count: number }>('/api/auth/sso/providers'),
        api.request<SsoStats>('/api/auth/sso/stats')
      ]);
      providers = providersResp.providers || [];
      stats = statsResp;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل البيانات';
    } finally {
      loading = false;
    }
  }

  function getLoginUrl(provider: SsoProvider): string {
    const kind = provider.kind === 'oidc' ? 'oidc' : 'saml';
    return `/api/auth/sso/${kind}/${provider.id}/login`;
  }

  onMount(loadData);
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">الدخول الموحد (SSO)</h1>
      <p class="text-sm text-zinc-400 mt-1">
        إدارة مزودي الهوية الخارجيين (OIDC + SAML)
      </p>
    </div>
    <button
      onclick={loadData}
      class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
    >
      تحديث
    </button>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">
      {error}
    </div>
  {/if}

  <!-- إحصائيات -->
  {#if stats}
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{stats.providers}</div>
        <div class="text-xs text-zinc-500 mt-1">المزودون</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{stats.pending_flows}</div>
        <div class="text-xs text-zinc-500 mt-1">تدفقات معلّقة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">{stats.active_sessions}</div>
        <div class="text-xs text-zinc-500 mt-1">جلسات نشطة</div>
      </div>
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div class="text-2xl font-bold text-white">
          {stats.flows_purged + stats.sessions_purged}
        </div>
        <div class="text-xs text-zinc-500 mt-1">منظّفة</div>
      </div>
    </div>
  {/if}

  <!-- قائمة المزودين -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    <div class="px-5 py-3 border-b border-zinc-800">
      <h2 class="text-sm font-semibold text-white">المزودون المُهيّأون</h2>
    </div>
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if providers.length === 0}
      <div class="p-8 text-center text-zinc-500">
        لا توجد مزودون مهيّأون. أضف مزوداً عبر API أو ملف التكوين.
      </div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each providers as p (p.id)}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-center justify-between">
              <div>
                <div class="flex items-center gap-2">
                  <h3 class="text-sm font-medium text-white">{p.display_name}</h3>
                  <span
                    class="px-2 py-0.5 text-xs rounded-full font-mono"
                    class:bg-purple-900={p.kind === 'oidc'}
                    class:text-purple-300={p.kind === 'oidc'}
                    class:bg-blue-900={p.kind === 'saml'}
                    class:text-blue-300={p.kind === 'saml'}
                  >
                    {p.kind.toUpperCase()}
                  </span>
                </div>
                <p class="text-xs text-zinc-500 mt-1">
                  المعرف: <code class="text-zinc-400">{p.id}</code>
                </p>
                <p class="text-xs text-zinc-500">
                  إعادة التوجيه: <code class="text-zinc-400">{p.redirect_after_login}</code>
                </p>
              </div>
              <a
                href={getLoginUrl(p)}
                class="px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white rounded-lg text-xs transition"
              >
                تسجيل الدخول
              </a>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- معلومات المسارات -->
  <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
    <h2 class="text-sm font-semibold text-white mb-3">مسارات SSO المتاحة</h2>
    <div class="space-y-2 text-xs font-mono">
      <div class="flex items-center gap-2">
        <span class="text-green-400">GET</span>
        <span class="text-zinc-300">/api/auth/sso/oidc/:provider/login</span>
        <span class="text-zinc-600">— بدء تدفق OIDC</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-green-400">GET</span>
        <span class="text-zinc-300">/api/auth/sso/oidc/:provider/callback</span>
        <span class="text-zinc-600">— استدعاء OIDC</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-green-400">GET</span>
        <span class="text-zinc-300">/api/auth/sso/saml/:provider/login</span>
        <span class="text-zinc-600">— بدء تدفق SAML</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-yellow-400">POST</span>
        <span class="text-zinc-300">/api/auth/sso/saml/:provider/acs</span>
        <span class="text-zinc-600">— استدعاء SAML ACS</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-green-400">GET</span>
        <span class="text-zinc-300">/api/auth/sso/providers</span>
        <span class="text-zinc-600">— قائمة المزودين</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-green-400">GET</span>
        <span class="text-zinc-300">/api/auth/sso/stats</span>
        <span class="text-zinc-600">— إحصائيات</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-yellow-400">POST</span>
        <span class="text-zinc-300">/api/auth/sso/logout</span>
        <span class="text-zinc-600">— إنهاء جلسة SSO</span>
      </div>
    </div>
  </div>
</div>
