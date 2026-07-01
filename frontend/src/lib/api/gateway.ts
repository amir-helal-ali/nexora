/**
 * Nexora Gateway API client.
 *
 * Wraps `fetch` with:
 * - JSON serialization
 * - Bearer token injection
 * - 401 → automatic redirect to /login
 * - Typed responses
 *
 * Per Part 7: NO business logic in the frontend. The frontend is a thin
 * real-time projection of Nexora Core. All state lives in the backend.
 */

const TOKEN_KEY = 'nexora.token';
const TOKEN_EXPIRY_KEY = 'nexora.token_expires_at_ns';

export interface LoginResponse {
  token: string;
  token_expires_at_ns: number;
  session_id: string;
  user_id: string;
  username: string;
}

export interface NxpEvent {
  id: number;
  name: string;
  payload: string;
  timestamp: number;
}

export interface Module {
  id: string;
  name: string;
  version: string;
  state: 'installed' | 'enabled' | 'paused' | 'removed';
  owner: string;
  capabilities: string[];
  installed_at: number;
  last_transition: number;
  transition_count: number;
}

export interface HealthSnapshot {
  ok: boolean;
  overall: string;
  subsystems: Array<{
    name: string;
    status: 'healthy' | 'degraded' | 'unhealthy';
    last_check: number;
    message: string | null;
  }>;
}

export interface PackageInfo {
  manifest: {
    id: string;
    name: string;
    version: string;
    package_type: string;
    owner_name: string;
    owner_public_key: string;
    capabilities: string[];
    billing: { kind: string; params?: Record<string, unknown> };
    visibility: string;
    description: string;
    readme: string;
    tags: string[];
    signature: string;
  };
  integrity_hash: string;
  published_at: number;
  install_count: number;
  active_install_count: number;
  installed: boolean;
  trust: {
    security: number;
    performance: number;
    stability: number;
    community_rating: number;
    enterprise_rating: number;
  };
}

export interface InstallReport {
  package_id: string;
  version: string;
  success: boolean;
  steps: Array<{
    step: number;
    name: string;
    passed: boolean;
    message: string | null;
    duration_us: number;
  }>;
  timestamp: number;
  error: string | null;
}

export interface NotificationItem {
  id: string;
  user_id: string;
  title: string;
  body: string;
  severity: 'info' | 'success' | 'warning' | 'error';
  read: boolean;
  created_at: number;
  link: string | null;
  icon: string | null;
}

export interface DashboardStats {
  ok: boolean;
  core: {
    modules: number;
    enabled_modules: number;
    events_published: number;
    principals: number;
    health: string;
  };
  marketplace: {
    total_packages: number;
  };
  billing: {
    invoice_count: number;
    payment_count: number;
    subscription_count: number;
    revenue_minor: number;
    outstanding_minor: number;
    currency: string;
  };
  workflow: {
    workflow_count: number;
    execution_count: number;
    succeeded: number;
    failed: number;
    stopped: number;
  };
  cluster: {
    total_nodes: number;
    healthy_nodes: number;
    degraded_nodes: number;
    unhealthy_nodes: number;
    offline_nodes: number;
  };
  notifications: {
    total: number;
  };
  timestamp: number;
}

export interface ApiError {
  ok: false;
  error: string;
}

function getToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string, expiresAtNs: number): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem(TOKEN_KEY, token);
  localStorage.setItem(TOKEN_EXPIRY_KEY, String(expiresAtNs));
}

export function clearToken(): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(TOKEN_EXPIRY_KEY);
}

export function isAuthenticated(): boolean {
  const token = getToken();
  if (!token) return false;
  const expiryStr = localStorage.getItem(TOKEN_EXPIRY_KEY);
  if (!expiryStr) return false;
  const expiry = Number(expiryStr);
  // Convert ns to ms for Date.now()
  const nowMs = Date.now();
  const expiryMs = expiry / 1_000_000;
  return nowMs < expiryMs;
}

export function getUsername(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem('nexora.username');
}

export function setUsername(username: string): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem('nexora.username', username);
}

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers = new Headers(options.headers);
  headers.set('Content-Type', 'application/json');

  const token = getToken();
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  const resp = await fetch(path, {
    ...options,
    headers,
  });

  if (resp.status === 401) {
    clearToken();
    if (typeof window !== 'undefined') {
      window.location.href = '/login';
    }
    throw new Error('Unauthorized');
  }

  const text = await resp.text();
  const body = text ? JSON.parse(text) : {};

  if (!resp.ok) {
    const err: ApiError = body;
    throw new Error(err.error || `HTTP ${resp.status}`);
  }

  return body as T;
}

export const api = {
  // ---- Raw request (for routes not covered by typed methods) ----
  async request<T = unknown>(path: string, options: RequestInit = {}): Promise<T> {
    return request<T>(path, options);
  },

  // ---- Auth ----
  async login(username: string, password: string): Promise<LoginResponse> {
    const resp = await request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    });
    setToken(resp.token, resp.token_expires_at_ns);
    setUsername(resp.username);
    return resp;
  },

  async logout(): Promise<void> {
    const token = getToken();
    if (!token) return;
    try {
      await request('/api/auth/logout', {
        method: 'POST',
        body: JSON.stringify({ token }),
      });
    } finally {
      clearToken();
    }
  },

  async refresh(): Promise<LoginResponse> {
    const token = getToken();
    if (!token) throw new Error('No token');
    const resp = await request<LoginResponse>('/api/auth/refresh', {
      method: 'POST',
      body: JSON.stringify({ token }),
    });
    setToken(resp.token, resp.token_expires_at_ns);
    return resp;
  },

  // ---- Core ----
  async ping(): Promise<{ pong: boolean }> {
    return request('/api/core/ping', { method: 'POST' });
  },

  async publishEvent(name: string, payload: string): Promise<{ event_id: number }> {
    return request('/api/core/events', {
      method: 'POST',
      body: JSON.stringify({ name, payload }),
    });
  },

  async replayEvents(fromId = 0, filter?: string): Promise<{ events: NxpEvent[] }> {
    const params = new URLSearchParams({ from_id: String(fromId) });
    if (filter) params.set('filter', filter);
    return request(`/api/core/events?${params}`);
  },

  async listModules(): Promise<{ ok: boolean; count: number; modules: Module[] }> {
    return request('/api/core/modules');
  },

  async getModule(id: string): Promise<{ ok: boolean; module: Module }> {
    return request(`/api/core/modules/${id}`);
  },

  async getHealth(): Promise<HealthSnapshot> {
    return request('/api/core/health');
  },

  // ---- Marketplace ----
  async listPackages(): Promise<{ ok: boolean; count: number; packages: PackageInfo[] }> {
    return request('/api/marketplace/packages');
  },

  async searchPackages(q: string): Promise<{ ok: boolean; count: number; packages: PackageInfo[] }> {
    return request(`/api/marketplace/packages/search?q=${encodeURIComponent(q)}`);
  },

  async getPackage(id: string): Promise<{ ok: boolean; package: PackageInfo }> {
    return request(`/api/marketplace/packages/${encodeURIComponent(id)}`);
  },

  async listInstalled(): Promise<{ ok: boolean; count: number; packages: PackageInfo[] }> {
    return request('/api/marketplace/installed');
  },

  async installPackage(id: string, version: string): Promise<{ ok: boolean; report: InstallReport }> {
    return request(`/api/marketplace/packages/${encodeURIComponent(id)}/install`, {
      method: 'POST',
      body: JSON.stringify({ version }),
    });
  },

  async uninstallPackage(id: string): Promise<{ ok: boolean }> {
    return request(`/api/marketplace/packages/${encodeURIComponent(id)}/uninstall`, {
      method: 'POST',
    });
  },

  // ---- Live event stream (SSE) ----
  /**
   * Open a Server-Sent Events stream for live events.
   * Returns an EventSource and an unsubscribe function.
   *
   * Per Part 7: "Real-time system: All updates are streaming-based."
   *
   * Note: EventSource doesn't support custom headers, so we pass the
   * Bearer token as a URL-encoded `?token=` query param. The gateway's
   * require_token middleware accepts this fallback for SSE endpoints only.
   */
  subscribeLiveEvents(
    onEvent: (evt: NxpEvent) => void,
    filter?: string,
  ): { close: () => void } {
    const token = getToken();
    const params = new URLSearchParams();
    if (filter) params.set('filter', filter);
    if (token) params.set('token', token); // URLSearchParams handles encoding
    const url = `/api/core/events/stream?${params.toString()}`;
    const es = new EventSource(url);

    es.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data);
        onEvent(data as NxpEvent);
      } catch {
        // ignore malformed
      }
    };

    es.addEventListener('error', () => {
      // EventSource auto-reconnects; nothing to do here
    });

    return {
      close: () => es.close(),
    };
  },

  // ---- Notifications ----
  async listNotifications(): Promise<{ ok: boolean; count: number; notifications: NotificationItem[] }> {
    return request('/api/notifications');
  },

  async unreadCount(): Promise<{ ok: boolean; count: number }> {
    return request('/api/notifications/unread_count');
  },

  async markNotificationRead(id: string): Promise<{ ok: boolean }> {
    return request(`/api/notifications/${encodeURIComponent(id)}/read`, { method: 'POST' });
  },

  async markAllNotificationsRead(): Promise<{ ok: boolean; marked_read: number }> {
    return request('/api/notifications/read_all', { method: 'POST' });
  },

  async deleteNotification(id: string): Promise<{ ok: boolean }> {
    return request(`/api/notifications/${encodeURIComponent(id)}`, { method: 'DELETE' });
  },

  // ---- Dashboard ----
  async getDashboardStats(): Promise<DashboardStats> {
    return request('/api/dashboard/stats');
  },

  // ---- System ----
  async getOpenApiSpec(): Promise<unknown> {
    return request('/api/openapi.json');
  },
};
