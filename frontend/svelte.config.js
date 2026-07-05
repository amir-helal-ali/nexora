import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      // إنتاج SPA ثابت مع fallback لـ index.html
      fallback: 'index.html',
      pages: 'build',
      assets: 'build',
      precompress: false,
      strict: false,
    }),
    alias: {
      $lib: './src/lib',
    },
  },
};

export default config;
