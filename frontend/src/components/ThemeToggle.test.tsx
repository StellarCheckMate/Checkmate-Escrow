import { fireEvent, render, screen } from '@testing-library/react';
import { ThemeToggle } from './ThemeToggle';

// WCAG AA requires a contrast ratio >= 4.5:1 for normal body text.
function relativeLuminance(hex: string): number {
  const c = hex.replace('#', '');
  const r = parseInt(c.slice(0, 2), 16) / 255;
  const g = parseInt(c.slice(2, 4), 16) / 255;
  const b = parseInt(c.slice(4, 6), 16) / 255;
  const [rl, gl, bl] = [r, g, b].map(v =>
    v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4),
  );
  return 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
}

function contrastRatio(hex1: string, hex2: string): number {
  const l1 = relativeLuminance(hex1);
  const l2 = relativeLuminance(hex2);
  const [lighter, darker] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (lighter + 0.05) / (darker + 0.05);
}

describe('ThemeToggle', () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  test('renders and toggles the theme attribute on <html>', () => {
    render(<ThemeToggle />);
    const button = screen.getByRole('button');

    fireEvent.click(button);
    const themeAfterFirstClick = document.documentElement.getAttribute('data-theme');
    expect(['light', 'dark']).toContain(themeAfterFirstClick);

    fireEvent.click(button);
    const themeAfterSecondClick = document.documentElement.getAttribute('data-theme');
    expect(themeAfterSecondClick).not.toBe(themeAfterFirstClick);
  });

  test('persists the chosen theme in localStorage', () => {
    render(<ThemeToggle />);
    fireEvent.click(screen.getByRole('button'));
    expect(window.localStorage.getItem('checkmate-escrow-theme')).toMatch(/^(light|dark)$/);
  });

  test('dark theme body text meets WCAG AA contrast against background', () => {
    // --text: #c3c8d1 on --bg: #16171d
    expect(contrastRatio('#c3c8d1', '#16171d')).toBeGreaterThanOrEqual(4.5);
  });

  test('dark theme heading text meets WCAG AA contrast against background', () => {
    // --text-h: #f3f4f6 on --bg: #16171d
    expect(contrastRatio('#f3f4f6', '#16171d')).toBeGreaterThanOrEqual(4.5);
  });

  test('light theme body text meets WCAG AA contrast against background', () => {
    // --text: #514a5b on --bg: #fff
    expect(contrastRatio('#514a5b', '#ffffff')).toBeGreaterThanOrEqual(4.5);
  });
});
