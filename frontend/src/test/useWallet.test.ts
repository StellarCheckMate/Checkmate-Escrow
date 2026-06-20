import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useWallet } from '../hooks/useWallet';

// Mock wallet adapters
vi.mock('../wallets/freighter', () => ({
  freighterIsAvailable: vi.fn(),
  freighterGetPublicKey: vi.fn(),
}));
vi.mock('../wallets/albedo', () => ({
  albedoIsAvailable: vi.fn(),
  albedoGetPublicKey: vi.fn(),
}));

import * as freighter from '../wallets/freighter';
import * as albedo from '../wallets/albedo';

const FAKE_KEY = 'GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';

beforeEach(() => vi.clearAllMocks());

describe('useWallet', () => {
  it('starts disconnected', () => {
    const { result } = renderHook(() => useWallet());
    expect(result.current.connected).toBe(false);
    expect(result.current.publicKey).toBeNull();
  });

  it('connects with Freighter', async () => {
    vi.mocked(freighter.freighterIsAvailable).mockResolvedValue(true);
    vi.mocked(freighter.freighterGetPublicKey).mockResolvedValue(FAKE_KEY);

    const { result } = renderHook(() => useWallet());
    await act(() => result.current.connect('freighter'));

    expect(result.current.connected).toBe(true);
    expect(result.current.publicKey).toBe(FAKE_KEY);
    expect(result.current.type).toBe('freighter');
    expect(result.current.error).toBeNull();
  });

  it('connects with Albedo', async () => {
    vi.mocked(albedo.albedoIsAvailable).mockReturnValue(true);
    vi.mocked(albedo.albedoGetPublicKey).mockResolvedValue(FAKE_KEY);

    const { result } = renderHook(() => useWallet());
    await act(() => result.current.connect('albedo'));

    expect(result.current.connected).toBe(true);
    expect(result.current.type).toBe('albedo');
  });

  it('sets error when Freighter not available', async () => {
    vi.mocked(freighter.freighterIsAvailable).mockResolvedValue(false);

    const { result } = renderHook(() => useWallet());
    await act(() => result.current.connect('freighter'));

    expect(result.current.connected).toBe(false);
    expect(result.current.error).toMatch(/Freighter/);
  });

  it('sets error on connect failure', async () => {
    vi.mocked(freighter.freighterIsAvailable).mockResolvedValue(true);
    vi.mocked(freighter.freighterGetPublicKey).mockRejectedValue(new Error('User rejected'));

    const { result } = renderHook(() => useWallet());
    await act(() => result.current.connect('freighter'));

    expect(result.current.connected).toBe(false);
    expect(result.current.error).toBe('User rejected');
  });

  it('disconnects', async () => {
    vi.mocked(freighter.freighterIsAvailable).mockResolvedValue(true);
    vi.mocked(freighter.freighterGetPublicKey).mockResolvedValue(FAKE_KEY);

    const { result } = renderHook(() => useWallet());
    await act(() => result.current.connect('freighter'));
    act(() => result.current.disconnect());

    expect(result.current.connected).toBe(false);
    expect(result.current.publicKey).toBeNull();
  });

  it('switches wallet without page reload', async () => {
    vi.mocked(freighter.freighterIsAvailable).mockResolvedValue(true);
    vi.mocked(freighter.freighterGetPublicKey).mockResolvedValue(FAKE_KEY);
    vi.mocked(albedo.albedoIsAvailable).mockReturnValue(true);
    vi.mocked(albedo.albedoGetPublicKey).mockResolvedValue('GALB' + 'E'.repeat(56 - 4));

    const { result } = renderHook(() => useWallet());
    await act(() => result.current.connect('freighter'));
    expect(result.current.type).toBe('freighter');

    await act(() => result.current.connect('albedo'));
    expect(result.current.type).toBe('albedo');
    expect(result.current.connected).toBe(true);
  });
});
