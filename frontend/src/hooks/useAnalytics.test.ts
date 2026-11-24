import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useAnalytics } from './useAnalytics';
import * as matchesHook from './useMatches';
import type { MatchSummary } from './useMatches';

vi.mock('./useMatches');

describe('useAnalytics', () => {
  const mockMatches: MatchSummary[] = [
    {
      match_id: 1,
      player1: 'GACCOUNT_A',
      player2: 'GACCOUNT_B',
      status: 'completed',
      winner: 'GACCOUNT_A',
      stake_amount: '100',
      token: 'XLM',
      timestamp: '2026-01-01T10:00:00Z',
    },
    {
      match_id: 2,
      player1: 'GACCOUNT_A',
      player2: 'GACCOUNT_B',
      status: 'completed',
      winner: 'GACCOUNT_B',
      stake_amount: '50',
      token: 'XLM',
      timestamp: '2026-01-02T10:00:00Z',
    },
    {
      match_id: 3,
      player1: 'GACCOUNT_B',
      player2: 'GACCOUNT_C',
      status: 'completed',
      winner: 'GACCOUNT_B',
      stake_amount: '200',
      token: 'XLM',
      timestamp: '2026-01-03T10:00:00Z',
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(matchesHook.useMatches).mockReturnValue({
      matches: mockMatches,
      loading: false,
      error: null,
      total: mockMatches.length,
    });
  });

  it('fetches overview on mount with connected account', () => {
    const { result } = renderHook(() => useAnalytics('GACCOUNT_A'));

    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
    expect(result.current.result).not.toBeNull();
    expect(result.current.result?.stats.totalMatches).toBe(2);
    expect(result.current.result?.stats.wins).toBe(1);
    expect(result.current.result?.stats.losses).toBe(1);
    expect(result.current.result?.stats.totalEarnings).toBe(50); // +100 - 50
  });

  it('refetches overview when the connected wallet account changes', () => {
    let account = 'GACCOUNT_A';
    const { result, rerender } = renderHook(
      ({ userAddress }) => useAnalytics(userAddress),
      { initialProps: { userAddress: account } },
    );

    expect(result.current.result?.stats.totalMatches).toBe(2);
    expect(result.current.result?.stats.wins).toBe(1);
    expect(result.current.result?.stats.losses).toBe(1);
    expect(result.current.result?.stats.totalEarnings).toBe(50);

    // Switch account to GACCOUNT_B
    rerender({ userAddress: 'GACCOUNT_B' });

    expect(result.current.result?.stats.totalMatches).toBe(3);
    expect(result.current.result?.stats.wins).toBe(2);
    expect(result.current.result?.stats.losses).toBe(1);
    expect(result.current.result?.stats.totalEarnings).toBe(150); // -100 + 50 + 200

    // Switch account to GACCOUNT_C
    rerender({ userAddress: 'GACCOUNT_C' });

    expect(result.current.result?.stats.totalMatches).toBe(1);
    expect(result.current.result?.stats.wins).toBe(0);
    expect(result.current.result?.stats.losses).toBe(1);
    expect(result.current.result?.stats.totalEarnings).toBe(-200);
  });

  it('clears or resets result when wallet account disconnects (null)', () => {
    const { result, rerender } = renderHook(
      ({ userAddress }: { userAddress: string | null }) => useAnalytics(userAddress),
      { initialProps: { userAddress: 'GACCOUNT_A' as string | null } },
    );

    expect(result.current.result).not.toBeNull();

    // Disconnect wallet
    rerender({ userAddress: null });

    expect(result.current.result).toBeNull();
  });
});
