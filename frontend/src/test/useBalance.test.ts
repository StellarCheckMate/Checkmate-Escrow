import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useBalance } from '../hooks/useBalance';

const mockLoadAccount = vi.fn();
const mockFetchTokenDecimals = vi.fn();

vi.mock('@stellar/stellar-sdk', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@stellar/stellar-sdk')>();
  return {
    ...actual,
    Horizon: {
      ...actual.Horizon,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      Server: vi.fn().mockImplementation(function (this: any) {
        this.loadAccount = mockLoadAccount;
      }),
    },
  };
});

vi.mock('../hooks/useTokenDecimals', () => ({
  fetchTokenDecimals: (...args: unknown[]) => mockFetchTokenDecimals(...args),
  resetTokenDecimalsCache: vi.fn(),
}));

describe('useBalance', () => {
  beforeEach(() => {
    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '42.0' }],
    });
    mockFetchTokenDecimals.mockResolvedValue(7);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    mockFetchTokenDecimals.mockReset();
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

  it('test_use_balance_divides_raw_balance_by_decimals', async () => {
    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '10000000' }],
    });
    mockFetchTokenDecimals.mockResolvedValue(7);

    const { result } = renderHook(() => useBalance('GABC123'));

    await act(async () => {});

    expect(mockFetchTokenDecimals).toHaveBeenCalledTimes(1);
    expect(result.current.balance).toBe('1');
  });

  it('test_use_balance_formats_with_correct_decimal_places', async () => {
    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '12345678' }],
    });
    mockFetchTokenDecimals.mockResolvedValue(7);

    const { result } = renderHook(() => useBalance('GABC123'));

    await act(async () => {});

    expect(result.current.balance).toBe('1.2345678');
  });

  it('test_use_balance_fetches_decimals_once_and_caches_across_polls', async () => {
    vi.useFakeTimers();

    mockLoadAccount.mockResolvedValue({
      balances: [{ asset_type: 'native', balance: '10000000' }],
    });
    mockFetchTokenDecimals.mockResolvedValue(7);

    const { result, unmount } = renderHook(() => useBalance('GABC123'));

    await act(async () => {});

    expect(mockFetchTokenDecimals).toHaveBeenCalledTimes(1);
    expect(result.current.balance).toBe('1');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(mockLoadAccount).toHaveBeenCalledTimes(2);
    expect(mockFetchTokenDecimals).toHaveBeenCalledTimes(1);
    expect(result.current.balance).toBe('1');

    unmount();
    vi.useRealTimers();
  });
});