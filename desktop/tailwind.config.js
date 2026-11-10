/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        zima: {
          darkest: '#09090b', // zinc-950
          darker: '#121215',
          card: '#18181b',    // zinc-900
          border: '#27272a',  // zinc-800
          accent: '#e94560',  // Zima signature red
          'accent-hover': '#ff5a77',
          blue: '#3b82f6',
          green: '#10b981',
          muted: '#71717a'
        }
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace']
      }
    }
  },
  plugins: []
};
