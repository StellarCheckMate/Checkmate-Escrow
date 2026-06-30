import { renderHook, waitFor } from '@testing-library/react';
import { act } from 'react-dom/test-utils';
import { useMatchStatus, Match, MatchState, Platform } from './useMatchStatus';

// Mock match data factory
const createMockMatch = (overrides?: Partial<Match>): Match => ({
  id: 1,
  player1: 'GABC123...',
  player2: 'GDEF456...',
  stake_amount: '1000000',
  token: 'USDC_TOKEN_ADDRESS',
  game_id: 'abcd1234',
  platform: Platform.Lichess,
  state: MatchState.Pending,
  player1_deposited: false,
  player2_deposited: false,
  created_ledger: 12345,
  ...overrides,
});

describe('useMatchStatus', () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.clearAllTimers();
    jest.useRealTimers();
  });

  describe('initial fetch', () => {
    it('should fetch match data on mount', async () => {
      const mockMatch = createMockMatch();
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 10000 })
      );

      expect(result.current.loading).toBe(true);
      expect(result.current.match).toBe(null);

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.match).toEqual(mockMatch);
      expect(result.current.error).toBe(null);
      expect(getMatchFn).toHaveBeenCalledWith(1);
      expect(getMatchFn).toHaveBeenCalledTimes(1);
    });

    it('should handle fetch errors', async () => {
      const mockError = new Error('Contract call failed');
      const getMatchFn = jest.fn().mockRejectedValue(mockError);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn)
      );

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.match).toBe(null);
      expect(result.current.error).toEqual(mockError);
    });

    it('should handle non-Error rejections', async () => {
      const getMatchFn = jest.fn().mockRejectedValue('String error');

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn)
      );

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error?.message).toBe('Failed to fetch match');
    });
  });

  describe('polling behavior', () => {
    it('should poll at the configured interval', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      // Initial fetch
      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(1);
      });

      // Advance time by 5 seconds
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(2);
      });

      // Advance time by another 5 seconds
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(3);
      });
    });

    it('should use default interval of 10 seconds', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      renderHook(() => useMatchStatus(1, getMatchFn));

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(1);
      });

      // Advance by 9 seconds - should not poll yet
      act(() => {
        jest.advanceTimersByTime(9000);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(1);

      // Advance by 1 more second - should poll
      act(() => {
        jest.advanceTimersByTime(1000);
      });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(2);
      });
    });

    it('should stop polling when match state is Completed', async () => {
      const activeMockMatch = createMockMatch({ state: MatchState.Active });
      const completedMockMatch = createMockMatch({ state: MatchState.Completed });
      
      const getMatchFn = jest
        .fn()
        .mockResolvedValueOnce(activeMockMatch)
        .mockResolvedValueOnce(completedMockMatch);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      // Initial fetch
      await waitFor(() => {
        expect(result.current.match?.state).toBe(MatchState.Active);
      });

      // Advance time - should poll again
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      // Wait for the second fetch to complete
      await waitFor(() => {
        expect(result.current.match?.state).toBe(MatchState.Completed);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(2);

      // Advance time - should NOT poll anymore
      act(() => {
        jest.advanceTimersByTime(10000);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(2);
    });

    it('should stop polling when match state is Cancelled', async () => {
      const pendingMockMatch = createMockMatch({ state: MatchState.Pending });
      const cancelledMockMatch = createMockMatch({ state: MatchState.Cancelled });
      
      const getMatchFn = jest
        .fn()
        .mockResolvedValueOnce(pendingMockMatch)
        .mockResolvedValueOnce(cancelledMockMatch);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      // Initial fetch
      await waitFor(() => {
        expect(result.current.match?.state).toBe(MatchState.Pending);
      });

      // Advance time - should poll again
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      // Wait for the second fetch to complete
      await waitFor(() => {
        expect(result.current.match?.state).toBe(MatchState.Cancelled);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(2);

      // Advance time - should NOT poll anymore
      act(() => {
        jest.advanceTimersByTime(10000);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(2);
    });

    it('should continue polling for Pending state', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Pending });
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(1);
      });

      // Should continue polling for Pending state
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(2);
      });

      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(3);
      });
    });

    it('should continue polling for Active state', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(1);
      });

      // Should continue polling for Active state
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(2);
      });
    });
  });

  describe('cleanup', () => {
    it('should cleanup interval on unmount', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      const { unmount } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(1);
      });

      unmount();

      // Advance time after unmount - should not poll
      act(() => {
        jest.advanceTimersByTime(10000);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(1);
    });

    it('should not update state after unmount', async () => {
      const mockMatch = createMockMatch();
      let resolveMatch: (value: Match) => void;
      const getMatchFn = jest.fn(
        () =>
          new Promise<Match>((resolve) => {
            resolveMatch = resolve;
          })
      );

      const { result, unmount } = renderHook(() =>
        useMatchStatus(1, getMatchFn)
      );

      expect(result.current.loading).toBe(true);

      unmount();

      // Resolve after unmount
      act(() => {
        resolveMatch!(mockMatch);
      });

      // State should not have updated
      expect(result.current.loading).toBe(true);
      expect(result.current.match).toBe(null);
    });
  });

  describe('matchId changes', () => {
    it('should refetch when matchId changes', async () => {
      const mockMatch1 = createMockMatch({ id: 1 });
      const mockMatch2 = createMockMatch({ id: 2 });
      const getMatchFn = jest
        .fn()
        .mockResolvedValueOnce(mockMatch1)
        .mockResolvedValueOnce(mockMatch2);

      const { result, rerender } = renderHook(
        ({ matchId }) => useMatchStatus(matchId, getMatchFn),
        { initialProps: { matchId: 1 } }
      );

      await waitFor(() => {
        expect(result.current.match?.id).toBe(1);
      });

      expect(getMatchFn).toHaveBeenCalledWith(1);

      // Change matchId
      rerender({ matchId: 2 });

      await waitFor(() => {
        expect(result.current.match?.id).toBe(2);
      });

      expect(getMatchFn).toHaveBeenCalledWith(2);
    });

    it('should clear old interval when matchId changes', async () => {
      const mockMatch1 = createMockMatch({ id: 1, state: MatchState.Active });
      const mockMatch2 = createMockMatch({ id: 2, state: MatchState.Active });
      const getMatchFn = jest.fn()
        .mockResolvedValue(mockMatch1)
        .mockResolvedValueOnce(mockMatch1)
        .mockResolvedValueOnce(mockMatch2)
        .mockResolvedValue(mockMatch2);

      const { rerender } = renderHook(
        ({ matchId }) => useMatchStatus(matchId, getMatchFn, { interval: 5000 }),
        { initialProps: { matchId: 1 } }
      );

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledWith(1);
      });

      const callsBeforeRerender = getMatchFn.mock.calls.length;

      // Change matchId
      rerender({ matchId: 2 });

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledWith(2);
      });

      // Advance time - should only poll for new matchId
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        const callsAfterTimer = getMatchFn.mock.calls.filter(
          (call) => call[0] === 2
        ).length;
        expect(callsAfterTimer).toBeGreaterThan(1);
      });

      // Should not have called with old matchId after rerender
      const callsForMatch1 = getMatchFn.mock.calls.filter(
        (call) => call[0] === 1
      ).length;
      expect(callsForMatch1).toBe(callsBeforeRerender);
    });
  });

  describe('enabled option', () => {
    it('should not fetch when enabled is false', async () => {
      const mockMatch = createMockMatch();
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { enabled: false })
      );

      expect(result.current.loading).toBe(false);
      expect(result.current.match).toBe(null);
      expect(getMatchFn).not.toHaveBeenCalled();

      // Advance time
      act(() => {
        jest.advanceTimersByTime(10000);
      });

      expect(getMatchFn).not.toHaveBeenCalled();
    });

    it('should start fetching when enabled changes to true', async () => {
      const mockMatch = createMockMatch();
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      const { result, rerender } = renderHook(
        ({ enabled }) => useMatchStatus(1, getMatchFn, { enabled }),
        { initialProps: { enabled: false } }
      );

      expect(getMatchFn).not.toHaveBeenCalled();

      rerender({ enabled: true });

      await waitFor(() => {
        expect(result.current.match).toEqual(mockMatch);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(1);
    });

    it('should stop fetching when enabled changes to false', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      const { rerender } = renderHook(
        ({ enabled }) => useMatchStatus(1, getMatchFn, { enabled }),
        { initialProps: { enabled: true } }
      );

      await waitFor(() => {
        expect(getMatchFn).toHaveBeenCalledTimes(1);
      });

      rerender({ enabled: false });

      act(() => {
        jest.advanceTimersByTime(20000);
      });

      // Should not have polled after being disabled
      expect(getMatchFn).toHaveBeenCalledTimes(1);
    });
  });

  describe('refetch function', () => {
    it('should provide a refetch function', async () => {
      const mockMatch = createMockMatch();
      const getMatchFn = jest.fn().mockResolvedValue(mockMatch);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn)
      );

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.refetch).toBeDefined();
      expect(typeof result.current.refetch).toBe('function');
    });

    it('should refetch when refetch is called', async () => {
      const mockMatch1 = createMockMatch({ state: MatchState.Pending });
      const mockMatch2 = createMockMatch({ state: MatchState.Active });
      const getMatchFn = jest
        .fn()
        .mockResolvedValueOnce(mockMatch1)
        .mockResolvedValueOnce(mockMatch2);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn)
      );

      await waitFor(() => {
        expect(result.current.match?.state).toBe(MatchState.Pending);
      });

      expect(getMatchFn).toHaveBeenCalledTimes(1);

      // Call refetch
      await act(async () => {
        await result.current.refetch();
      });

      expect(getMatchFn).toHaveBeenCalledTimes(2);
      expect(result.current.match?.state).toBe(MatchState.Active);
    });

    it('should set loading to true during refetch', async () => {
      const mockMatch = createMockMatch();
      let resolveMatch: (value: Match) => void;
      const getMatchFn = jest
        .fn()
        .mockResolvedValueOnce(mockMatch)
        .mockImplementationOnce(
          () =>
            new Promise<Match>((resolve) => {
              resolveMatch = resolve;
            })
        );

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn)
      );

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      // Start refetch
      act(() => {
        result.current.refetch();
      });

      // Should be loading
      await waitFor(() => {
        expect(result.current.loading).toBe(true);
      });

      // Resolve the refetch
      await act(async () => {
        resolveMatch!(mockMatch);
      });

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });
    });
  });

  describe('error handling during polling', () => {
    it('should handle errors during polling', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const mockError = new Error('Network error');
      const getMatchFn = jest
        .fn()
        .mockResolvedValueOnce(mockMatch)
        .mockRejectedValueOnce(mockError);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      await waitFor(() => {
        expect(result.current.match).toEqual(mockMatch);
        expect(result.current.error).toBe(null);
      });

      // Advance time to trigger poll
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(result.current.error).toEqual(mockError);
      });
    });

    it('should clear previous errors on successful fetch', async () => {
      const mockMatch = createMockMatch({ state: MatchState.Active });
      const mockError = new Error('Network error');
      const getMatchFn = jest
        .fn()
        .mockRejectedValueOnce(mockError)
        .mockResolvedValueOnce(mockMatch);

      const { result } = renderHook(() =>
        useMatchStatus(1, getMatchFn, { interval: 5000 })
      );

      await waitFor(() => {
        expect(result.current.error).toEqual(mockError);
      });

      // Advance time to trigger poll
      act(() => {
        jest.advanceTimersByTime(5000);
      });

      await waitFor(() => {
        expect(result.current.error).toBe(null);
        expect(result.current.match).toEqual(mockMatch);
      });
    });
  });
});
