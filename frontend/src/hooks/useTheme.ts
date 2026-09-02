import { useEffect, useState } from 'react';

export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'checkmate-escrow-theme';

function getSystemPreference(): Theme {
  if (typeof window === 'undefined' || !window.matchMedia) return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function getStoredTheme(): Theme | null {
  if (typeof window === 'undefined') return null;
  const stored = window.localStorage.getItem(STORAGE_KEY);
  return stored === 'dark' || stored === 'light' ? stored : null;
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => getStoredTheme() ?? getSystemPreference());

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
    window.localStorage.setItem(STORAGE_KEY, theme);
  }, [theme]);

  function toggleTheme() {
    setTheme(t => (t === 'dark' ? 'light' : 'dark'));
  }

  return { theme, setTheme, toggleTheme };
}
