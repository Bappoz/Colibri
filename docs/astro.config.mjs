import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';

// Site de documentação do roadmap da engine Colibri.
// Design proposital: simples, leve, focado em leitura.
export default defineConfig({
  integrations: [mdx()],
  markdown: {
    // Tema duplo: o Shiki emite as duas cores como CSS vars; o global.css
    // troca conforme prefers-color-scheme (senão o código fica ilegível no dark).
    shikiConfig: {
      themes: { light: 'github-light', dark: 'github-dark' },
      wrap: false,
    },
  },
});
