/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        nexo: {
          bg: '#0b1020',
          panel: '#121a2e',
          cyan: '#00e5ff',
          cyanDim: '#1a6b78',
          green: '#00e676',
          amber: '#ffb300',
          red: '#ff5252',
          muted: '#7a8aa8'
        }
      },
      boxShadow: {
        glow: '0 0 24px rgba(0,229,255,0.35)',
        glowGreen: '0 0 24px rgba(0,230,118,0.45)'
      }
    }
  },
  plugins: []
};
