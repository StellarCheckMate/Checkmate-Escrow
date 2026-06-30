import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import { WalletConnector } from '../components/wallet/WalletConnector';
import { BalanceDisplay } from '../components/wallet/BalanceDisplay';
import { TransactionSigner } from '../components/wallet/TransactionSigner';
import type { WalletType } from '../wallets/types';

vi.mock('../wallets/freighter', () => ({ freighterSign: vi.fn() }));
vi.mock('../wallets/albedo', () => ({ albedoSign: vi.fn() }));
vi.mock('../hooks/useBalance', () => ({
  useBalance: () => ({ balance: '100.0000000', loading: false, error: null }),
}));

const makeWallet = (overrides = {}) => ({
  type: null as WalletType | null,
  publicKey: null as string | null,
  connected: false,
  error: null as string | null,
  connect: vi.fn(),
  disconnect: vi.fn(),
  ...overrides,
});

describe('Snapshots', () => {
  it('WalletConnector disconnected', () => {
    const { container } = render(<WalletConnector wallet={makeWallet()} />);
    expect(container).toMatchSnapshot();
  });

  it('WalletConnector connected', () => {
    const wallet = makeWallet({
      connected: true,
      type: 'freighter' as WalletType,
      publicKey: 'GABCDE1234567890ABCDE1234567890ABCDE1234567890ABCDE1234567',
    });
    const { container } = render(<WalletConnector wallet={wallet} />);
    expect(container).toMatchSnapshot();
  });

  it('WalletConnector with error', () => {
    const { container } = render(
      <WalletConnector wallet={makeWallet({ error: 'Freighter extension not detected.' })} />
    );
    expect(container).toMatchSnapshot();
  });

  it('BalanceDisplay', () => {
    const { container } = render(
      <BalanceDisplay publicKey="GABCDE1234567890ABCDE1234567890ABCDE1234567890ABCDE1234567" />
    );
    expect(container).toMatchSnapshot();
  });

  it('TransactionSigner idle', () => {
    const { container } = render(
      <TransactionSigner walletType="freighter" xdr="AAAA==" onSigned={vi.fn()} />
    );
    expect(container).toMatchSnapshot();
  });

  it('TransactionSigner no wallet', () => {
    const { container } = render(
      <TransactionSigner walletType={null} xdr="AAAA==" onSigned={vi.fn()} />
    );
    expect(container).toMatchSnapshot();
  });
});
