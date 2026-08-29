import { useMatch } from '../../hooks/useMatch';
import { MatchDetailSkeleton } from './MatchDetailSkeleton';
import { MatchStatusBadge } from './MatchStatusBadge';

interface MatchDetailProps {
  matchId: number | null;
}

export function MatchDetail({ matchId }: MatchDetailProps) {
  const { match, loading, error } = useMatch(matchId);

  if (loading) {
    return <MatchDetailSkeleton />;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (!match) {
    return <p>No match selected.</p>;
  }

  return (
    <div className="match-detail">
      <h2>Match #{match.match_id}</h2>
      <div className="match-detail-players">
        <span>{match.player1}</span>
        <span> vs </span>
        <span>{match.player2}</span>
      </div>
      <MatchStatusBadge status={match.status as 'pending' | 'active' | 'completed' | 'cancelled'} />
      {match.stake_amount && <p>Stake: {match.stake_amount} {match.token}</p>}
      {match.winner && <p>Winner: {match.winner}</p>}
    </div>
  );
}
