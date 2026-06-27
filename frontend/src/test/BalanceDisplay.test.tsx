import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { BalanceDisplay } from '../components/wallet/BalanceDisplay';

vi.mock('../hooks/useBalance', () => ({
  useBalance: vi.fn(),
}));

import { useBalance } from '../hooks/useBalance';

describe('BalanceDisplay', () => {
  it('renders loading state', () => {
    vi.mocked(useBalance).mockReturnValue({ balance: null, loading: true, error: null });

    render(<BalanceDisplay publicKey="GABC" />);

    expect(screen.getByText('Loading balance…')).toBeInTheDocument();
  });

  it('renders balance value', () => {
    vi.mocked(useBalance).mockReturnValue({ balance: '100.5', loading: false, error: null });

    render(<BalanceDisplay publicKey="GABC" />);

    expect(screen.getByText('100.5 XLM')).toBeInTheDocument();
  });

  it('renders error state', () => {
    vi.mocked(useBalance).mockReturnValue({ balance: null, loading: false, error: 'Failed to fetch' });

    render(<BalanceDisplay publicKey="GABC" />);

    expect(screen.getByRole('alert')).toHaveTextContent('Failed to fetch');
  });
});