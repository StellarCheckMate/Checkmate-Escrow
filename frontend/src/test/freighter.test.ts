import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@stellar/freighter-api', () => ({
  isConnected: vi.fn(),
  getAddress: vi.fn(),
  signTransaction: vi.fn(),
}));

import { isConnected, getAddress, signTransaction } from '@stellar/freighter-api';
import {
  freighterIsAvailable,
  freighterGetPublicKey,
  freighterSign,
} from '../wallets/freighter';

const FAKE_PUBKEY = 'GFREIGHTER1234567890ABCDE1234567890ABCDE1234567890ABCDE12345';

beforeEach(() => vi.clearAllMocks());

describe('freighterIsAvailable', () => {
  it('test_freighter_is_available_returns_true_when_connected', async () => {
    vi.mocked(isConnected).mockResolvedValue({ isConnected: true } as Awaited<ReturnType<typeof isConnected>>);
    const result = await freighterIsAvailable();
    expect(result).toBe(true);
  });

  it('test_freighter_is_available_returns_false_when_not_connected', async () => {
    vi.mocked(isConnected).mockResolvedValue({ isConnected: false } as Awaited<ReturnType<typeof isConnected>>);
    const result = await freighterIsAvailable();
    expect(result).toBe(false);
  });

  it('test_freighter_is_available_returns_false_on_error', async () => {
    vi.mocked(isConnected).mockRejectedValue(new Error('Extension not found'));
    const result = await freighterIsAvailable();
    expect(result).toBe(false);
  });
});

describe('freighterGetPublicKey', () => {
  it('test_freighter_get_public_key_returns_address_on_success', async () => {
    vi.mocked(getAddress).mockResolvedValue({
      address: FAKE_PUBKEY,
      error: undefined,
    } as Awaited<ReturnType<typeof getAddress>>);
    const key = await freighterGetPublicKey();
    expect(key).toBe(FAKE_PUBKEY);
  });

  it('test_freighter_get_public_key_throws_when_error_present', async () => {
    vi.mocked(getAddress).mockResolvedValue({
      address: '',
      error: { message: 'User rejected the request', code: 4001 },
    } as Awaited<ReturnType<typeof getAddress>>);
    await expect(freighterGetPublicKey()).rejects.toThrow('User rejected the request');
  });

  it('test_freighter_get_public_key_calls_getAddress', async () => {
    vi.mocked(getAddress).mockResolvedValue({
      address: FAKE_PUBKEY,
      error: undefined,
    } as Awaited<ReturnType<typeof getAddress>>);
    await freighterGetPublicKey();
    expect(getAddress).toHaveBeenCalledTimes(1);
  });
});

describe('freighterSign', () => {
  const FAKE_XDR = 'AAAA...fake_xdr...ZZZZ';
  const FAKE_NETWORK = 'Test SDF Network ; September 2015';
  const FAKE_SIGNED_XDR = 'BBBB...signed_xdr...YYYY';

  it('test_freighter_sign_returns_signed_xdr_on_success', async () => {
    vi.mocked(signTransaction).mockResolvedValue({
      signedTxXdr: FAKE_SIGNED_XDR,
      error: undefined,
    } as Awaited<ReturnType<typeof signTransaction>>);
    const result = await freighterSign(FAKE_XDR, FAKE_NETWORK);
    expect(result.signedXdr).toBe(FAKE_SIGNED_XDR);
  });

  it('test_freighter_sign_passes_network_passphrase', async () => {
    vi.mocked(signTransaction).mockResolvedValue({
      signedTxXdr: FAKE_SIGNED_XDR,
      error: undefined,
    } as Awaited<ReturnType<typeof signTransaction>>);
    await freighterSign(FAKE_XDR, FAKE_NETWORK);
    expect(signTransaction).toHaveBeenCalledWith(FAKE_XDR, {
      networkPassphrase: FAKE_NETWORK,
    });
  });

  it('test_freighter_sign_throws_when_error_present', async () => {
    vi.mocked(signTransaction).mockResolvedValue({
      signedTxXdr: '',
      error: { message: 'Transaction rejected', code: 4001 },
    } as Awaited<ReturnType<typeof signTransaction>>);
    await expect(freighterSign(FAKE_XDR, FAKE_NETWORK)).rejects.toThrow(
      'Transaction rejected',
    );
  });
});
