import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useMatch } from '../hooks/useMatch';

describe('useMatch', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('fetches match info and refreshes every 10 seconds', async () => {
    vi.useFakeTimers();

    const matchResponse = {
      success: true,
      data: {
        match_id: 1,
        player1: 'GPLAYER1',
        player2: 'GPLAYER2',
        status: 'active',
      },
      error: null,
    };

    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => matchResponse,
    });

    vi.stubGlobal('fetch', fetchMock);

    const { result } = renderHook(() => useMatch(1));

    await waitFor(() => expect(result.current.match).toEqual(matchResponse.data));
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
    expect(fetchMock).toHaveBeenCalledWith('http://localhost:8080/match/1');

    vi.advanceTimersByTime(10_000);

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  });

  it('sets error when matchId is null', () => {
    const { result } = renderHook(() => useMatch(null));
    expect(result.current.match).toBeNull();
    expect(result.current.error).toBeNull();
    expect(result.current.loading).toBe(false);
  });
});
