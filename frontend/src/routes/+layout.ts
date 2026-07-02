import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';
import { isAuthenticated } from '$lib/api/gateway';

export const load: LayoutLoad = async ({ url }) => {
  const path = url.pathname;
  const publicPaths = ['/login'];
  const isPublic = publicPaths.includes(path);

  if (!isPublic && !isAuthenticated()) {
    throw redirect(302, '/login');
  }
  if (path === '/login' && isAuthenticated()) {
    throw redirect(302, '/');
  }

  return {};
};

export const prerender = false;
export const ssr = false;
