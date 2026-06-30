/**
 * Svelte store for the current user's session state.
 */
import { writable } from 'svelte/store';
import { getUsername, isAuthenticated } from '$lib/api/gateway';

export interface SessionState {
  username: string | null;
  authenticated: boolean;
}

function createSessionStore() {
  const initial: SessionState = {
    username: getUsername(),
    authenticated: isAuthenticated(),
  };
  const { subscribe, set, update } = writable<SessionState>(initial);

  return {
    subscribe,
    setAuthenticated: (username: string) =>
      set({ username, authenticated: true }),
    clear: () => set({ username: null, authenticated: false }),
    refresh: () =>
      update(() => ({
        username: getUsername(),
        authenticated: isAuthenticated(),
      })),
  };
}

export const session = createSessionStore();
