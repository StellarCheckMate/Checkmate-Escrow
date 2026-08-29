import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AdminPanel } from '../pages/AdminPanel';
import * as useAdminContractModule from '../hooks/useAdminContract';

const ADMIN_ADDRESS = 'GAKNDFRRWA3RPWNQJWWPRLCJNUHHL3MCLCHHNRGJA7GIILUFOLSTMBWM';
const TREASURY_ADDRESS = 'GBXJIIGB7V5K4OQZNWUXIHZBVPTH3YLMZ7PPJZB3KMIIGYVPQTUNPLZE';
const FEE_RECIPIENT_ADDRESS = 'GC7VOKLUM7SBRXGKTMDN4TXQY5F5F2QSQKBTLLGB6NBIMOTMDXZQK5AI';

const wallet = {
  connected: true,
  publicKey: ADMIN_ADDRESS,
  type: 'freighter' as const,
  error: null,
  connect: vi.fn(),
  disconnect: vi.fn(),
};

describe('AdminPanel', () => {
  it('fetches protocol config on mount and pre-populates form fields', () => {
    const spy = vi.spyOn(useAdminContractModule, 'useAdminContract').mockReturnValue({
      admin: ADMIN_ADDRESS,
      oracle: 'GORACLEXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX',
      paused: false,
      protocolConfig: {
        vestingDurationSeconds: 86_400,
        cancellationFeeBasisPoints: 50,
        treasury: TREASURY_ADDRESS,
        stablecoinOnlyMode: true,
        matchTimeoutSeconds: 604_800,
        protocolFeeBps: 250,
        feeRecipient: FEE_RECIPIENT_ADDRESS,
        minimumStake: '1000000',
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

    render(<AdminPanel wallet={wallet} />);

    expect(screen.getByLabelText('Treasury address')).toHaveValue(TREASURY_ADDRESS);
    expect(screen.getByLabelText('Fee recipient address')).toHaveValue(FEE_RECIPIENT_ADDRESS);
    expect(screen.getByLabelText('Protocol fee (bps)')).toHaveValue(250);
    expect(screen.getByLabelText('Vesting duration (seconds)')).toHaveValue(86_400);
    expect(screen.getByLabelText('Match timeout (seconds)')).toHaveValue(604_800);
    expect(screen.getByLabelText('Cancellation fee (bps)')).toHaveValue(50);
    expect(screen.getByLabelText('Minimum stake')).toHaveValue('1000000');
    expect(screen.getByLabelText('Stablecoin-only mode')).toBeChecked();

    spy.mockRestore();
  });
});
