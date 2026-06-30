import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTransaction } from '../hooks/useTransaction';

vi.mock('../wallets/freighter', () => ({
  freighterSign: vi.fn(),
}));
vi.mock('../wallets/albedo', () => ({
  albedoSign: vi.fn(),
}));

import * as freighter from '../wallets/freighter';
import * as albedo from '../wallets/albedo';

const FAKE_XDR = 'AAAA==';
const SIGNED_XDR = 'BBBB==';

beforeEach(() => vi.clearAllMocks());

describe('useTransaction', () => {
  it('returns null when no wallet connected', async () => {
    const { result } = renderHook(() => useTransaction(null));
    let signed: string | null = 'initial';
    await act(async () => { signed = await result.current.sign(FAKE_XDR); });
    expect(signed).toBeNull();
    expect(result.current.error).toMatch(/No wallet/);
  });

  it('signs with Freighter', async () => {
    vi.mocked(freighter.freighterSign).mockResolvedValue({ signedXdr: SIGNED_XDR });
    const { result } = renderHook(() => useTransaction('freighter'));
    let signed: string | null = null;
    await act(async () => { signed = await result.current.sign(FAKE_XDR); });
    expect(signed).toBe(SIGNED_XDR);
    expect(result.current.signing).toBe(false);
  });

  it('signs with Albedo', async () => {
    vi.mocked(albedo.albedoSign).mockResolvedValue({ signedXdr: SIGNED_XDR });
    const { result } = renderHook(() => useTransaction('albedo'));
    let signed: string | null = null;
    await act(async () => { signed = await result.current.sign(FAKE_XDR); });
    expect(signed).toBe(SIGNED_XDR);
  });

  it('captures signing errors', async () => {
    vi.mocked(freighter.freighterSign).mockRejectedValue(new Error('Rejected'));
    const { result } = renderHook(() => useTransaction('freighter'));
    let signed: string | null = 'x';
    await act(async () => { signed = await result.current.sign(FAKE_XDR); });
    expect(signed).toBeNull();
    expect(result.current.error).toBe('Rejected');
    expect(result.current.loading).toBe(false);
  });

  it('test_use_transaction_error_state', async () => {
    vi.mocked(freighter.freighterSign).mockRejectedValue(new Error('Transaction rejected'));
    const { result } = renderHook(() => useTransaction('freighter'));
    let signed: string | null = 'x';

    await act(async () => { signed = await result.current.sign(FAKE_XDR); });

    expect(signed).toBeNull();
    expect(result.current.error).toBe('Transaction rejected');
    expect(result.current.loading).toBe(false);
  });
});
