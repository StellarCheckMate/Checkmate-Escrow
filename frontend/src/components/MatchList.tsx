import { MatchCard } from './match/MatchCard';

interface Match {
  matchId: number;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  status: 'pending' | 'active' | 'completed' | 'cancelled';
  platform: 'lichess' | 'chessdotcom';
}

interface MatchListProps {
  matches: Match[];
  loading?: boolean;
  error?: string | null;
}

export function MatchList({ matches, loading = false, error = null }: MatchListProps) {
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
