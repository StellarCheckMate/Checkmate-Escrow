import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { TransactionSigner } from '../components/wallet/TransactionSigner';

vi.mock('../wallets/freighter', () => ({ freighterSign: vi.fn() }));
vi.mock('../wallets/albedo', () => ({ albedoSign: vi.fn() }));

import * as freighter from '../wallets/freighter';

beforeEach(() => vi.clearAllMocks());

describe('TransactionSigner', () => {
  it('is disabled with no wallet', () => {
    render(<TransactionSigner walletType={null} xdr="AAAA==" onSigned={vi.fn()} />);
    expect(screen.getByRole('button')).toBeDisabled();
  });

  it('calls onSigned with result', async () => {
    vi.mocked(freighter.freighterSign).mockResolvedValue({ signedXdr: 'SIGNED==' });
    const onSigned = vi.fn();
    render(<TransactionSigner walletType="freighter" xdr="AAAA==" onSigned={onSigned} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => expect(onSigned).toHaveBeenCalledWith('SIGNED=='));
    expect(screen.getByText('Signed ✓')).toBeInTheDocument();
  });

  it('shows error on failure', async () => {
    vi.mocked(freighter.freighterSign).mockRejectedValue(new Error('User rejected'));
    render(<TransactionSigner walletType="freighter" xdr="AAAA==" onSigned={vi.fn()} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('User rejected'));
  });
});
