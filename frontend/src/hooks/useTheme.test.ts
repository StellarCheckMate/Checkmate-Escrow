import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, beforeEach } from 'vitest';
import { useTheme } from './useTheme';

const STORAGE_KEY = 'checkmate-escrow-theme';

// jsdom does not implement matchMedia, so stub the system preference the
// hook falls back to when no theme is stored.
function stubSystemPreference(dark: boolean) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (window as any).matchMedia = (query: string) => ({
    matches: dark && query === '(prefers-color-scheme: dark)',
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  });
}

describe('useTheme localStorage persistence', () => {
  beforeEach(() => {
    window.localStorage.clear();
    stubSystemPreference(false); // system preference: light
  });

  describe('reads the theme from localStorage on mount', () => {
    it('uses the stored dark theme on mount', () => {
      window.localStorage.setItem(STORAGE_KEY, 'dark');

      const { result } = renderHook(() => useTheme());

      expect(result.current.theme).toBe('dark');
    });

    it('uses the stored light theme on mount, even when the system prefers dark', () => {
      stubSystemPreference(true);
      window.localStorage.setItem(STORAGE_KEY, 'light');

      const { result } = renderHook(() => useTheme());

      expect(result.current.theme).toBe('light');
    });

    it('falls back to the system preference when nothing is stored', () => {
      stubSystemPreference(true);

      const { result } = renderHook(() => useTheme());

      expect(result.current.theme).toBe('dark');
    });
  });

  describe('writes the chosen theme to localStorage', () => {
    it('persists the theme when toggled', () => {
      const { result } = renderHook(() => useTheme());

      act(() => {
        result.current.toggleTheme();
      });
      expect(result.current.theme).toBe('dark');
      expect(window.localStorage.getItem(STORAGE_KEY)).toBe('dark');

      act(() => {
        result.current.toggleTheme();
      });
      expect(result.current.theme).toBe('light');
      expect(window.localStorage.getItem(STORAGE_KEY)).toBe('light');
    });

    it('persists the theme when set directly', () => {
      const { result } = renderHook(() => useTheme());

      act(() => {
        result.current.setTheme('dark');
      });

      expect(window.localStorage.getItem(STORAGE_KEY)).toBe('dark');
    });

    it('writes the active theme to localStorage on mount', () => {
      renderHook(() => useTheme());

      expect(window.localStorage.getItem(STORAGE_KEY)).toBe('light');
    });
  });
});
