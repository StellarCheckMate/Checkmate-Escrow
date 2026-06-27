import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useBalance } from '../hooks/useBalance';

const FAKE_KEY = 'GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';

const mockLoadAccount = vi.fn();

vi.mock('@stellar/stellar-sdk', () => {
  function MockServer() {
    return { loadAccount: mockLoadAccount };
  }
  return {
    SorobanRpc: {},
    Asset: {},
    Horizon: {
      Server: MockServer,
    },
  };
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useBalance', () => {
  it('returns null balance when publicKey is null', () => {
    const { result } = renderHook(() => useBalance(null));
    expect(result.current.balance).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('fetches XLM balance for a valid public key', async () => {
    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '42.5000000' }],
    });

    const { result } = renderHook(() => useBalance(FAKE_KEY));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.balance).toBe('42.5000000');
    expect(result.current.error).toBeNull();
  });

  it('sets error when loadAccount rejects', async () => {
    mockLoadAccount.mockRejectedValue(new Error('Network error'));

    const { result } = renderHook(() => useBalance(FAKE_KEY));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe('Network error');
    expect(result.current.balance).toBeNull();
  });

  it('test_use_balance_cleanup_on_unmount: clears interval and stops fetching after unmount', async () => {
    vi.useFakeTimers();

    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '10.0000000' }],
    });

    const { unmount } = renderHook(() => useBalance(FAKE_KEY));

    // Allow the initial fetch to complete
    await act(() => vi.advanceTimersByTimeAsync(0));

    const callsAfterMount = mockLoadAccount.mock.calls.length;
    expect(callsAfterMount).toBeGreaterThanOrEqual(1);

    // Unmount — this should invoke the cleanup, calling clearInterval
    unmount();

    // Advance time well past the 10-second interval threshold
    await act(() => vi.advanceTimersByTimeAsync(30_000));

    // No additional fetches should have been triggered after unmount
    expect(mockLoadAccount.mock.calls.length).toBe(callsAfterMount);
  });
});
