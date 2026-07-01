import { useEffect, useState, useRef, useCallback } from 'react';

/**
 * Match states from the Soroban contract
 */
export enum MatchState {
  Pending = 'Pending',
  Active = 'Active',
  Completed = 'Completed',
  Cancelled = 'Cancelled',
}

/**
 * Platform enum from the contract
 */
export enum Platform {
  Lichess = 'Lichess',
  ChessDotCom = 'ChessDotCom',
}

/**
 * Winner enum from the contract
 */
export enum Winner {
  Player1 = 'Player1',
  Player2 = 'Player2',
  Draw = 'Draw',
}

/**
 * Match structure from the Soroban contract
 */
export interface Match {
  id: number;
  player1: string;
  player2: string;
  stake_amount: string;
  token: string;
  game_id: string;
  platform: Platform;
  state: MatchState;
  player1_deposited: boolean;
  player2_deposited: boolean;
  created_ledger: number;
}

/**
 * Configuration options for useMatchStatus hook
 */
export interface UseMatchStatusOptions {
  /**
   * Polling interval in milliseconds
   * @default 10000 (10 seconds)
   */
  interval?: number;
  
  /**
   * Whether to start polling immediately
   * @default true
   */
  enabled?: boolean;
}

/**
 * Return type for useMatchStatus hook
 */
export interface UseMatchStatusReturn {
  match: Match | null;
  loading: boolean;
  error: Error | null;
  refetch: () => Promise<void>;
}

/**
 * Function type for fetching match data from the contract
 */
export type GetMatchFunction = (matchId: number) => Promise<Match>;

/**
 * Custom React hook that polls for match state changes from the Soroban contract.
 * 
 * This hook automatically:
 * - Polls the contract at a configurable interval (default 10s)
 * - Stops polling when match state reaches Completed or Cancelled
 * - Cleans up intervals on unmount
 * - Handles loading and error states
 * 
 * @param matchId - The ID of the match to monitor
 * @param getMatchFn - Function that fetches match data from the contract
 * @param options - Configuration options for polling behavior
 * @returns Object containing match data, loading state, error state, and refetch function
 * 
 * @example
 * ```tsx
 * const { match, loading, error } = useMatchStatus(
 *   matchId,
 *   async (id) => {
 *     const result = await contract.get_match({ match_id: id });
 *     return result;
 *   },
 *   { interval: 5000 } // Poll every 5 seconds
 * );
 * 
 * if (loading) return <div>Loading...</div>;
 * if (error) return <div>Error: {error.message}</div>;
 * if (match?.state === MatchState.Completed) {
 *   return <div>Match completed!</div>;
 * }
 * ```
 */
export function useMatchStatus(
  matchId: number,
  getMatchFn: GetMatchFunction,
  options: UseMatchStatusOptions = {}
): UseMatchStatusReturn {
  const { interval = 10000, enabled = true } = options;

  const [match, setMatch] = useState<Match | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<Error | null>(null);

  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const isMountedRef = useRef<boolean>(true);

  /**
   * Determines if polling should stop based on match state
   */
  const shouldStopPolling = useCallback((matchState: MatchState | null): boolean => {
    if (!matchState) return false;
    return matchState === MatchState.Completed || matchState === MatchState.Cancelled;
  }, []);

  /**
   * Fetches match data from the contract
   */
  const fetchMatch = useCallback(async () => {
    if (!enabled) return;

    try {
      setError(null);
      const matchData = await getMatchFn(matchId);
      
      // Only update state if component is still mounted
      if (isMountedRef.current) {
        setMatch(matchData);
        setLoading(false);

        // Stop polling if match has reached a terminal state
        if (shouldStopPolling(matchData.state) && intervalRef.current) {
          clearInterval(intervalRef.current);
          intervalRef.current = null;
        }
      }
    } catch (err) {
      if (isMountedRef.current) {
        setError(err instanceof Error ? err : new Error('Failed to fetch match'));
        setLoading(false);
      }
    }
  }, [matchId, getMatchFn, enabled, shouldStopPolling]);

  /**
   * Manual refetch function that can be called by consumers
   */
  const refetch = useCallback(async () => {
    setLoading(true);
    await fetchMatch();
  }, [fetchMatch]);

  // Initial fetch and polling setup
  useEffect(() => {
    if (!enabled) {
      setLoading(false);
      return;
    }

    // Fetch immediately on mount or when matchId changes
    fetchMatch();

    // Set up polling interval if match is not in terminal state
    if (!shouldStopPolling(match?.state ?? null)) {
      intervalRef.current = setInterval(() => {
        fetchMatch();
      }, interval);
    }

    // Cleanup function
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [matchId, interval, enabled, fetchMatch, match?.state, shouldStopPolling]);

  // Cleanup on unmount
  useEffect(() => {
    isMountedRef.current = true;

    return () => {
      isMountedRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, []);

  return {
    match,
    loading,
    error,
    refetch,
  };
}
