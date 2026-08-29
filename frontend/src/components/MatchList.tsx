import { MatchCard } from './match/MatchCard';
import { useMatchWebSocket, type IndexedEvent } from '../hooks/useMatchWebSocket';

interface Match {
  matchId: number;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  status: 'pending' | 'active' | 'completed' | 'cancelled';
  platform: 'lichess' | 'chessdotcom';
}

/** Event types that mean the currently displayed match list is stale. */
const REFRESH_EVENT_TYPES = new Set([
  'deposit',
  'deposit_confirmed',
  'result_submitted',
  'match_completed',
  'match_cancelled',
]);

interface MatchListProps {
  matches: Match[];
  loading?: boolean;
  error?: string | null;
  /**
   * Called whenever a deposit or oracle result-submission event arrives for
   * one of the currently displayed matches, so the parent can refetch.
   */
  onRefresh?: () => void;
}

export function MatchList({ matches, loading = false, error = null, onRefresh }: MatchListProps) {
  const matchIds = matches.map(m => m.matchId);

  const handleEvent = (event: IndexedEvent) => {
    if (!onRefresh) return;
    if (REFRESH_EVENT_TYPES.has(event.event_type)) {
      onRefresh();
    }
  };

  useMatchWebSocket({
    matchIds,
    enabled: Boolean(onRefresh) && matchIds.length > 0,
    onEvent: handleEvent,
  });

  if (loading) {
    return <p role="status" aria-live="polite">Loading matches…</p>;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (matches.length === 0) {
    return <p>No matches found.</p>;
  }

  return (
    <ul aria-label="Match list">
      {matches.map(match => (
        <li key={match.matchId} tabIndex={0}>
          <MatchCard {...match} />
        </li>
      ))}
    </ul>
  );
}
