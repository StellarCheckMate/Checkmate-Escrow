import { useState, useEffect } from 'react';

const EVENT_INDEXER_URL = import.meta.env.VITE_EVENT_INDEXER_URL ?? 'http://localhost:8080';

export interface MatchInfo {
  match_id: number;
  player1: string;
  player2: string;
  status: string;
  winner?: string;
  stake_amount?: string;
  token?: string;
  game_id?: string;
  platform?: string;
  created_ledger?: number;
  completed_ledger?: number;
  events?: Array<Record<string, unknown>>;
}

interface ApiResponse<T> {
  success: boolean;
  data: T | null;
  error: string | null;
}

export function useMatch(matchId: number | null) {
  const [match, setMatch] = useState<MatchInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (matchId === null) {
      setMatch(null);
      setError(null);
      setLoading(false);
      return;
    }

    let cancelled = false;
    let interval: ReturnType<typeof setInterval> | null = null;

    async function fetchMatch() {
      setLoading(true);
      setError(null);

      try {
        const response = await fetch(`${EVENT_INDEXER_URL}/match/${matchId}`);
        if (!response.ok) {
          throw new Error(`Failed to fetch match: ${response.statusText}`);
        }

        const body = (await response.json()) as ApiResponse<MatchInfo>;
        if (cancelled) return;

        if (!body.success || !body.data) {
          setError(body.error ?? 'Match not found');
          setMatch(null);
        } else {
          setMatch(body.data);
        }
      } catch (err) {
        if (cancelled) return;
        setError((err as Error).message);
        setMatch(null);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    fetchMatch();
    interval = setInterval(fetchMatch, 10_000);

    return () => {
      cancelled = true;
      if (interval) clearInterval(interval);
    };
  }, [matchId]);

  return { match, loading, error };
}
