import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AdminPanel } from '../pages/AdminPanel';
import * as adminHook from '../hooks/useAdminContract';

vi.mock('../hooks/useAdminContract', () => ({
  useAdminContract: vi.fn(),
}));

describe('AdminPanel', () => {
  const mockWallet = {
    type: 'freighter' as const,
    publicKey: 'GAKNDFRRWA3RPWNQJWWPRLCJNUHHL3MCLCHHNRGJA7GIILUFOLSTMBWM',
    connected: true,
    error: null,
    connect: vi.fn(),
    disconnect: vi.fn(),
  };

  it('loads and pre-populates current protocol config values', () => {
    vi.mocked(adminHook.useAdminContract).mockReturnValue({
      admin: 'GAKNDFRRWA3RPWNQJWWPRLCJNUHHL3MCLCHHNRGJA7GIILUFOLSTMBWM',
      oracle: 'GORACLE123',
      paused: false,
      protocolConfig: {
        protocol_fee_bps: 75,
        minimum_stake: 10,
        maximum_stake: 5000,
        treasury: 'GTREASURY999',
        fee_recipient: 'GFEERECIPIENT888',
        cancellation_fee_basis_points: 150,
      },
      loading: false,
      error: null,
      isAdmin: true,
      actionLoading: false,
      actionError: null,
      refresh: vi.fn(),
      pause: vi.fn(),
      unpause: vi.fn(),
      addToken: vi.fn(),
      removeToken: vi.fn(),
      rotateOracle: vi.fn(),
      transferAdmin: vi.fn(),
    });

    render(<AdminPanel wallet={mockWallet} />);

    expect(screen.getByRole('heading', { name: /Protocol Configuration/i })).toBeInTheDocument();

    expect(screen.getByLabelText(/Protocol Fee \(bps\)/i)).toHaveValue(75);
    expect(screen.getByLabelText(/Minimum Stake/i)).toHaveValue(10);
    expect(screen.getByLabelText(/Maximum Stake/i)).toHaveValue(5000);
    expect(screen.getByLabelText(/Treasury Address/i)).toHaveValue('GTREASURY999');
    expect(screen.getByLabelText(/Fee Recipient/i)).toHaveValue('GFEERECIPIENT888');
    expect(screen.getByLabelText(/Cancellation Fee \(bps\)/i)).toHaveValue(150);
  });
});
