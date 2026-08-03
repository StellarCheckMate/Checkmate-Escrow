import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useMatch } from '../hooks/useMatch';

describe('useMatch', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns loading=true while the fetch is in-flight', async () => {
    // Never resolves so the hook stays in the loading state.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockReturnValue(new Promise(() => {})),
    );

    const { result } = renderHook(() => useMatch(1));

    // Before the promise settles the hook must signal that it is loading.
    expect(result.current.loading).toBe(true);
    expect(result.current.match).toBeNull();
    expect(result.current.error).toBeNull();
  });

  it('populates match data and clears loading on a successful fetch', async () => {
    const matchData = {
      match_id: 42,
      player1: 'GPLAYER1',
      player2: 'GPLAYER2',
      status: 'active',
      stake_amount: '100',
      token: 'GTOKEN',
      game_id: 'abc123',
      platform: 'Lichess',
    };

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ success: true, data: matchData, error: null }),
    }));

    const { result } = renderHook(() => useMatch(42));

    await act(async () => {});

    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
    expect(result.current.match).toEqual(matchData);
  });

  it('sets an error message and clears match data when the fetch fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      statusText: 'Internal Server Error',
    }));

    const { result } = renderHook(() => useMatch(99));

    await act(async () => {});

    expect(result.current.loading).toBe(false);
    expect(result.current.match).toBeNull();
    expect(result.current.error).toBe('Failed to fetch match: Internal Server Error');
  });

  it('sets error when the API returns success=false', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ success: false, data: null, error: 'match not found' }),
    }));

    const { result } = renderHook(() => useMatch(5));

    await act(async () => {});

    expect(result.current.match).toBeNull();
    expect(result.current.error).toBe('match not found');
  });

  it('re-fetches when the matchId changes', async () => {
    const makeResponse = (id: number) => ({
      ok: true,
      json: async () => ({
        success: true,
        data: { match_id: id, player1: 'GA', player2: 'GB', status: 'pending' },
        error: null,
      }),
    });

    const fetchMock = vi.fn()
      .mockResolvedValueOnce(makeResponse(1))
      .mockResolvedValueOnce(makeResponse(2));

    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(({ id }: { id: number }) => useMatch(id), {
      initialProps: { id: 1 },
    });

    await act(async () => {});
    expect(result.current.match?.match_id).toBe(1);

    // Change the matchId — the hook must re-run the effect and fetch again.
    rerender({ id: 2 });
    await act(async () => {});

    expect(result.current.match?.match_id).toBe(2);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenNthCalledWith(1, 'http://localhost:8080/match/1');
    expect(fetchMock).toHaveBeenNthCalledWith(2, 'http://localhost:8080/match/2');
  });

  it('returns null match with no error or loading when matchId is null', () => {
    const { result } = renderHook(() => useMatch(null));

    expect(result.current.match).toBeNull();
    expect(result.current.error).toBeNull();
    expect(result.current.loading).toBe(false);
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

    await act(async () => {});
    expect(result.current.match).toEqual(matchResponse.data);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
    expect(fetchMock).toHaveBeenCalledWith('http://localhost:8080/match/1');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
