<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api/gateway';

  interface MfaStatus {
    enrolled: boolean;
    enabled: boolean;
  }

  interface MfaEnrollment {
    secret: string;
    otpauth_url: string;
    backup_codes: string[];
  }

  let status: MfaStatus | null = null;
  let enrollment: MfaEnrollment | null = null;
  let verifyCode = '';
  let loading = true;
  let error = '';
  let message = '';

  async function loadStatus() {
    loading = true;
    try {
      status = await api.request<MfaStatus>('/api/auth/mfa/status');
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل تحميل الحالة';
    } finally {
      loading = false;
    }
  }

  async function beginEnrollment() {
    error = '';
    message = '';
    try {
      enrollment = await api.request<MfaEnrollment>('/api/auth/mfa/enroll/begin', { method: 'POST' });
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل بدء التفعيل';
    }
  }

  async function completeEnrollment() {
    if (!enrollment) return;
    if (!verifyCode || verifyCode.length !== 6) {
      error = 'أدخل رمزاً من 6 أرقام';
      return;
    }
    error = '';
    try {
      await api.request('/api/auth/mfa/enroll/complete', {
        method: 'POST',
        body: JSON.stringify({
          code: verifyCode,
          secret: enrollment.secret,
          backup_codes: enrollment.backup_codes
        })
      });
      enrollment = null;
      verifyCode = '';
      message = 'تم تفعيل MFA بنجاح';
      await loadStatus();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل إكمال التفعيل';
    }
  }

  async function disableMfa() {
    if (!confirm('هل أنت متأكد من تعطيل MFA؟')) return;
    error = '';
    try {
      await api.request('/api/auth/mfa/disable', { method: 'POST' });
      message = 'تم تعطيل MFA';
      await loadStatus();
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التعطيل';
    }
  }

  async function regenerateBackupCodes() {
    if (!confirm('سيتم استبدال أكواد الاسترداد القديمة. متابعة؟')) return;
    error = '';
    try {
      const resp = await api.request<{ backup_codes: string[] }>(
        '/api/auth/mfa/backup-codes/regenerate',
        { method: 'POST' }
      );
      enrollment = { secret: '', otpauth_url: '', backup_codes: resp.backup_codes };
      message = 'تم توليد أكواد استرداد جديدة';
    } catch (e) {
      error = e instanceof Error ? e.message : 'فشل التوليد';
    }
  }

  onMount(loadStatus);
</script>

<div class="space-y-6">
  <div>
    <h1 class="text-2xl font-bold text-white">المصادقة متعددة العوامل (MFA)</h1>
    <p class="text-sm text-zinc-400 mt-1">حماية إضافية لحسابك عبر TOTP</p>
  </div>

  {#if error}
    <div class="bg-red-900/50 border border-red-700 text-red-200 px-4 py-3 rounded-lg text-sm">
      {error}
    </div>
  {/if}

  {#if message}
    <div class="bg-green-900/50 border border-green-700 text-green-200 px-4 py-3 rounded-lg text-sm">
      {message}
    </div>
  {/if}

  {#if loading}
    <div class="p-8 text-center text-zinc-500">جارٍ التحميل...</div>
  {:else if status?.enabled}
    <!-- MFA مُفعّل -->
    <div class="bg-green-900/30 border border-green-700 rounded-xl p-6">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 bg-green-600 rounded-full flex items-center justify-center text-white text-xl">✓</div>
        <div>
          <h2 class="text-lg font-semibold text-white">MFA مُفعّل</h2>
          <p class="text-sm text-zinc-400">حسابك محمي بالمصادقة متعددة العوامل</p>
        </div>
      </div>
      <div class="mt-4 flex gap-3">
        <button
          onclick={regenerateBackupCodes}
          class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
        >
          توليد أكواد استرداد جديدة
        </button>
        <button
          onclick={disableMfa}
          class="px-4 py-2 bg-red-900/50 hover:bg-red-800 text-red-200 rounded-lg text-sm transition"
        >
          تعطيل MFA
        </button>
      </div>
    </div>
  {:else if enrollment}
    <!-- عملية التفعيل -->
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-4">
      <h2 class="text-lg font-semibold text-white">تفعيل MFA</h2>
      <div class="space-y-3">
        <div>
          <span class="text-sm text-zinc-400 block">1. امسح هذا السر في تطبيق المصادقة:</span>
          <div class="mt-2 bg-zinc-800 p-3 rounded-lg">
            <code class="text-green-400 text-sm break-all">{enrollment.secret}</code>
          </div>
        </div>
        {#if enrollment.otpauth_url}
          <div>
            <span class="text-sm text-zinc-400 block">أو امسح QR code:</span>
            <div class="mt-2 bg-zinc-800 p-3 rounded-lg">
              <code class="text-blue-400 text-xs break-all">{enrollment.otpauth_url}</code>
            </div>
          </div>
        {/if}
        <div>
          <span class="text-sm text-zinc-400 block">2. أدخل الرمز من 6 أرقام:</span>
          <input
            bind:value={verifyCode}
            placeholder="000000"
            maxlength="6"
            class="mt-2 w-48 bg-zinc-800 text-white text-center text-2xl tracking-widest px-3 py-2 rounded-lg border border-zinc-700"
          />
        </div>
        {#if enrollment.backup_codes?.length}
          <div>
            <span class="text-sm text-zinc-400 block">أكواد الاسترداد (احفظها بأمان):</span>
            <div class="mt-2 grid grid-cols-2 gap-2">
              {#each enrollment.backup_codes as code}
                <code class="bg-zinc-800 px-2 py-1 rounded text-yellow-400 text-sm font-mono">{code}</code>
              {/each}
            </div>
          </div>
        {/if}
      </div>
      <div class="flex gap-3">
        <button
          onclick={completeEnrollment}
          class="px-4 py-2 bg-green-600 hover:bg-green-500 text-white rounded-lg text-sm transition"
        >
          تأكيد التفعيل
        </button>
        <button
          onclick={() => { enrollment = null; verifyCode = ''; }}
          class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded-lg text-sm transition"
        >
          إلغاء
        </button>
      </div>
    </div>
  {:else}
    <!-- MFA غير مُفعّل -->
    <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
      <div class="flex items-center gap-3 mb-4">
        <div class="w-10 h-10 bg-zinc-700 rounded-full flex items-center justify-center text-zinc-400 text-xl">!</div>
        <div>
          <h2 class="text-lg font-semibold text-white">MFA غير مُفعّل</h2>
          <p class="text-sm text-zinc-400">فعّل المصادقة متعددة العوامل لحماية حسابك</p>
        </div>
      </div>
      <button
        onclick={beginEnrollment}
        class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-sm transition"
      >
        بدء التفعيل
      </button>
    </div>
  {/if}
</div>
