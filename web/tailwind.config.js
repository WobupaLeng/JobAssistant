/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // PRD 3.1 指定配色
        light: {
          bg: '#F3F6F9',
          card: '#FFFFFF',
        },
        dark: {
          bg: '#0F172A',
          card: '#1E293B',
          border: '#334155',
        },
        primary: '#10B981', // 翡翠绿
        secondary: '#2563EB', // 科技蓝
      },
      fontFamily: {
        sans: [
          'system-ui',
          '-apple-system',
          'Segoe UI',
          'Microsoft YaHei',
          'sans-serif',
        ],
      },
    },
  },
  plugins: [],
}
