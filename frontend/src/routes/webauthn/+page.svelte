<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface Credential {
    id: string;
    public_key: string;
    authenticator_type: string;
    label: string;
    created_at: number;
    last_used_at: number | null;
    sign_count: number;
  }

  let credentials: Credential[] = [];
  let registered = false;
  let loading = true;
  let error = '';
  let message = '';
  let showRegister = false;
  let newLabel = '';
  let newAuthType = 'yubikey';
  let registering = false;

  async function loadCredentials() {
    loading = true;
    try {
      const resp = await api.request<{ credentials: Credential[]; count: number; registered: boolean }>(
        '/api/auth/webauthn/credentials'
      );
      credentials = resp.credentials || [];
      registered = resp.registered;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل المفاتيح';
    } finally {
      loading = false;
    }
  }

  async function beginRegistration() {
    error = '';
    message = '';
    registering = true;
    try {
      const resp = await api.request<{ challenge: string; expires_in_seconds: number }>(
        '/api/auth/webauthn/register/begin',
        { method: 'POST' }
      );
      message = `تحدي جاهز! المس مفتاح الأمان ثم أكمل التسجيل.`;
      showRegister = true;
      // في الإنتاج، نمرر challenge إلى WebAuthn API في المتصفح.
      // للتنفيذ المرجعي، نطلب من المستخدم إدخال البيانات يدوياً.
      window.challenge = resp.challenge;
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل بدء التسجيل';
    } finally {
      registering = false;
    }
  }

  async function completeRegistration() {
    if (!newLabel) {
      error = 'أدخل اسماً وصفي للمفتاح';
      return;
    }
    error = '';
    try {
      // في الإنتاج، نحصل على هذه البيانات من WebAuthn API.
      // للتنفيذ المرجعي، نولّد قيم وهمية.
      const credId = crypto.randomUUID();
      const pubKey = btoa(Math.random().toString(36).slice(2));

      await api.request('/api/auth/webauthn/register/complete', {
        method: 'POST',
        body: JSON.stringify({
          credential_id: credId,
          public_key: pubKey,
          authenticator_type: newAuthType,
          label: newLabel
        })
      });
      showRegister = false;
      newLabel = '';
      message = 'تم تسجيل المفتاح الأمني بنجاح';
      await loadCredentials();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل إكمال التسجيل';
    }
  }

  async function deleteCredential(id: string) {
    if (!confirm('هل أنت متأكد من حذف هذا المفتاح الأمني؟')) return;
    error = '';
    try {
      await api.request(`/api/auth/webauthn/credentials/${id}`, { method: 'DELETE' });
      message = 'تم حذف المفتاح';
      await loadCredentials();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل الحذف';
    }
  }

  function formatTime(ns: number | null): string {
    if (!ns) return '—';
    return new Date(ns / 1_000_000).toLocaleString('ar');
  }

  onMount(loadCredentials);
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold text-white">مفاتيح الأمان (WebAuthn)</h1>
      <p class="text-sm text-zinc-400 mt-1">مفاتيح مادية مثل YubiKey و Google Titan</p>
    </div>
    <button
      onclick={loadCredentials}
      class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
    >
      تحديث
    </button>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">{error}</div>
  {/if}

  {#if message}
    <div class="bg-green-900/50 border border-green-700 text-green-200 px-4 py-3 rounded-lg text-sm">{message}</div>
  {/if}

  {#if registered}
    <div class="bg-green-900/30 border border-green-700 rounded-xl p-4 flex items-center gap-3">
      <span class="text-green-400 text-xl">✓</span>
      <span class="text-green-200 text-sm">WebAuthn مُفعّل — لديك {credentials.length} مفتاح مسجّل</span>
    </div>
  {:else}
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4 flex items-center gap-3">
      <span class="text-zinc-500 text-xl">!</span>
      <span class="text-zinc-400 text-sm">WebAuthn غير مُفعّل — أضف مفتاح أمان للحماية الإضافية</span>
    </div>
  {/if}

  {#if !showRegister}
    <button
      onclick={beginRegistration}
      disabled={registering}
      class="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded-lg text-sm transition"
    >
      {registering ? 'جارٍ التحضير...' : '+ إضافة مفتاح أمان'}
    </button>
  {/if}

  {#if showRegister}
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 space-y-3">
      <h2 class="text-sm font-semibold text-white">تسجيل مفتاح أمان جديد</h2>
      <div>
        <label class="text-sm text-zinc-400">اسم وصفي للمفتاح:</label>
        <input
          bind:value={newLabel}
          placeholder="مثلاً: مفتاحي الأساسي"
          class="mt-1 w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
        />
      </div>
      <div>
        <label class="text-sm text-zinc-400">نوع المنشئ:</label>
        <select
          bind:value={newAuthType}
          class="mt-1 w-full bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-700 text-sm"
        >
          <option value="yubikey">YubiKey</option>
          <option value="titan">Google Titan</option>
          <option value="windows-hello">Windows Hello</option>
          <option value="touch-id">Touch ID</option>
          <option value="android">Android</option>
          <option value="other">أخرى</option>
        </select>
      </div>
      <div class="flex gap-3">
        <button
          onclick={completeRegistration}
          class="px-4 py-2 bg-green-600 hover:bg-green-500 text-white rounded-lg text-sm transition"
        >
          تأكيد التسجيل
        </button>
        <button
          onclick={() => { showRegister = false; newLabel = ''; }}
          class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
        >
          إلغاء
        </button>
      </div>
    </div>
  {/if}

  <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
    {#if loading}
      <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
    {:else if credentials.length === 0}
      <div class="p-8 text-center text-zinc-500">لا توجد مفاتيح أمان مسجّلة</div>
    {:else}
      <div class="divide-y divide-zinc-800">
        {#each credentials as c (c.id)}
          <div class="p-4 hover:bg-zinc-800/50 transition">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-white">{c.label}</span>
                  <span class="px-2 py-0.5 text-xs bg-zinc-700 text-zinc-300 rounded-full">{c.authenticator_type}</span>
                </div>
                <div class="mt-1 text-xs text-zinc-500 flex gap-4">
                  <span>التسجيل: {formatTime(c.created_at)}</span>
                  <span>آخر استخدام: {formatTime(c.last_used_at)}</span>
                  <span>عدد الاستخدامات: {c.sign_count}</span>
                </div>
              </div>
              <button
                onclick={() => deleteCredential(c.id)}
                class="px-2 py-1 text-xs bg-red-900/50 hover:bg-red-800 text-red-200 rounded transition"
              >
                حذف
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
