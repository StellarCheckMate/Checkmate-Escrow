import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@albedo-link/intent', () => ({
  default: {
    publicKey: vi.fn(),
  },
}));

import albedo from '@albedo-link/intent';
import { albedoIsAvailable, albedoGetPublicKey } from '../wallets/albedo';

const FAKE_PUBKEY = 'GALBEDO1234567890ABCDE1234567890ABCDE1234567890ABCDE1234567890';

beforeEach(() => vi.clearAllMocks());

describe('albedo', () => {
  it('test_albedo_is_available_browser', () => {
    expect(albedoIsAvailable()).toBe(true);
  });

  it('test_albedo_get_public_key_success', async () => {
    vi.mocked(albedo.publicKey).mockResolvedValue({ pubkey: FAKE_PUBKEY });
    const key = await albedoGetPublicKey();
    expect(key).toBe(FAKE_PUBKEY);
    expect(albedo.publicKey).toHaveBeenCalledWith({});
  });

  it('test_albedo_get_public_key_throws', async () => {
    vi.mocked(albedo.publicKey).mockRejectedValue(new Error('User rejected'));
    await expect(albedoGetPublicKey()).rejects.toThrow('User rejected');
  });
});
