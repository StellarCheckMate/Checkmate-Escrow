import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import App from '../App';

// Stub out hooks and page components so the landing page renders in isolation
vi.mock('../hooks/useWallet', () => ({
  useWallet: () => ({
    connected: false,
    publicKey: null,
    type: null,
    error: null,
    connect: vi.fn(),
    disconnect: vi.fn(),
  }),
}));

vi.mock('../pages/AdminPanel', () => ({
  AdminPanel: () => <div data-testid="admin-panel" />,
}));

vi.mock('../pages/MatchDetailPage', () => ({
  MatchDetailPage: () => <div data-testid="match-detail-page" />,
}));

describe('App landing page', () => {
  it('renders the ThemeToggle on the landing page', () => {
    render(<App />);
    // ThemeToggle renders a button with aria-label containing "mode"
    const toggle = screen.getByRole('button', { name: /mode/i });
    expect(toggle).toBeTruthy();
  });
});
