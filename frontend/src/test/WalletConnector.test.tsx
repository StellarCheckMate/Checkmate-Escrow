import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import { WalletConnector } from '../components/wallet/WalletConnector';
import { useWallet } from '../hooks/useWallet';
import type { WalletState, WalletType } from '../wallets/types';

vi.mock('../wallets/freighter', () => ({
  freighterIsAvailable: vi.fn(),
  freighterGetPublicKey: vi.fn(),
}));

import * as freighter from '../wallets/freighter';

const makeWallet = (overrides: Partial<WalletState> = {}) => ({
  type: null as WalletType | null,
  publicKey: null as string | null,
  connected: false,
  error: null as string | null,
  connect: vi.fn(),
  disconnect: vi.fn(),
  ...overrides,
});

function WalletConnectorWithHook() {
  const wallet = useWallet();
  return <WalletConnector wallet={wallet} />;
}

describe('WalletConnector', () => {
  it('renders connect buttons when disconnected', () => {
    render(<WalletConnector wallet={makeWallet()} />);
    expect(screen.getByText('Connect Freighter')).toBeInTheDocument();
    expect(screen.getByText('Connect Albedo')).toBeInTheDocument();
  });

  it('calls connect with correct wallet type', () => {
    const wallet = makeWallet();
    render(<WalletConnector wallet={wallet} />);
    fireEvent.click(screen.getByText('Connect Freighter'));
    expect(wallet.connect).toHaveBeenCalledWith('freighter');
    fireEvent.click(screen.getByText('Connect Albedo'));
    expect(wallet.connect).toHaveBeenCalledWith('albedo');
  });

  it('shows truncated key and disconnect when connected', () => {
    const key = 'GABCDE1234567890ABCDE1234567890ABCDE1234567890ABCDE1234567';
    const wallet = makeWallet({ connected: true, publicKey: key, type: 'freighter' });
    render(<WalletConnector wallet={wallet} />);
    expect(screen.getByText(/GABCDE/)).toBeInTheDocument();
    expect(screen.getByText('Disconnect')).toBeInTheDocument();
  });

  it('calls disconnect', () => {
    const key = 'GABCDE1234567890ABCDE1234567890ABCDE1234567890ABCDE1234567';
    const wallet = makeWallet({ connected: true, publicKey: key, type: 'freighter' });
    render(<WalletConnector wallet={wallet} />);
    fireEvent.click(screen.getByText('Disconnect'));
    expect(wallet.disconnect).toHaveBeenCalled();
  });

  it('shows error message', () => {
    const wallet = makeWallet({ error: 'Freighter extension not detected.' });
    render(<WalletConnector wallet={wallet} />);
    expect(screen.getByRole('alert')).toHaveTextContent('Freighter extension not detected.');
  });

  it('test_wallet_connector_freighter_not_available', async () => {
    vi.mocked(freighter.freighterIsAvailable).mockResolvedValue(false);
    render(<WalletConnectorWithHook />);

    fireEvent.click(screen.getByText('Connect Freighter'));

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent('Freighter extension not detected');
    });
  });
});
