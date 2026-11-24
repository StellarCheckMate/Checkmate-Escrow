import { useState, useEffect } from 'react';

const EVENT_INDEXER_URL = import.meta.env.VITE_EVENT_INDEXER_URL ?? 'http://localhost:8080';

type MatchStatus = 'pending' | 'active' | 'completed' | 'cancelled' | 'expired';

export interface MatchSummary {
  match_id: number;
  player1: string;
  player2: string;
  status: string;
  winner?: string;
  stake_amount?: string;
  token?: string;
  game_id?: string;
  platform?: string;
  timestamp?: string;
  event_type?: string;
  ledger_sequence?: number;
}

const EMPTY_MATCHES: MatchSummary[] = [];

interface UseMatchesOptions {
  status?: MatchStatus;
  limit?: number;
  offset?: number;
  /** Refetch the shared feed when the consuming account changes. Null disables fetching. */
  account?: string | null;
}

interface ApiResponse<T> {
  success: boolean;
  data: T | null;
  error: string | null;
}

export function useMatches({ status, limit = 100, offset = 0, account }: UseMatchesOptions = {}) {
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [settledAccount, setSettledAccount] = useState<string | null>();

  useEffect(() => {
    if (account === null) return;
    let cancelled = false;

    async function fetchMatches() {
      setLoading(true);
      setError(null);

      try {
        const url = new URL(`${EVENT_INDEXER_URL}/events`);
        if (status) url.searchParams.set('status', status);
        url.searchParams.set('limit', String(limit));
        url.searchParams.set('offset', String(offset));

        const response = await fetch(url.toString());
        if (!response.ok) {
          throw new Error(`Failed to load matches: ${response.statusText}`);
        }

        const body = (await response.json()) as ApiResponse<MatchSummary[]>;
        if (cancelled) return;

        if (!body.success || !body.data) {
          setError(body.error ?? 'Failed to load matches.');
          setMatches([]);
          setTotal(0);
        } else {
          setMatches(body.data);
          setTotal(body.data.length);
        }
      } catch (err) {
        if (cancelled) return;
        setError((err as Error).message);
        setMatches([]);
        setTotal(0);
      } finally {
        if (!cancelled) {
          setSettledAccount(account);
          setLoading(false);
        }
      }
    }

    fetchMatches();

    return () => {
      cancelled = true;
    };
  }, [status, limit, offset, account]);

  const awaitingAccount = account !== undefined && account !== settledAccount;
  if (awaitingAccount) {
    return { matches: EMPTY_MATCHES, loading: account !== null, error: null, total: 0 };
  }
  return { matches, loading, error, total };
}
