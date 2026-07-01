import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useBalance } from '../hooks/useBalance';

const mockLoadAccount = vi.fn();

vi.mock('@stellar/stellar-sdk', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@stellar/stellar-sdk')>();
  return {
    ...actual,
    Horizon: {
      ...actual.Horizon,
      Server: vi.fn().mockImplementation(function (this: any) {
        this.loadAccount = mockLoadAccount;
      }),
    },
  };
});

describe('useBalance', () => {
  beforeEach(() => {
    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '42.0' }],
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('test_use_balance_polls_every_10_seconds', async () => {
    vi.useFakeTimers();

    const { unmount } = renderHook(() => useBalance('GABC123'));
    
    // Let React run effects and resolve the mock loadAccount
    await act(async () => {});

    expect(mockLoadAccount).toHaveBeenCalledTimes(1);

    // Advance time by 10 seconds to trigger the interval
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(mockLoadAccount).toHaveBeenCalledTimes(2);

    unmount();
    vi.useRealTimers();
  });
});
