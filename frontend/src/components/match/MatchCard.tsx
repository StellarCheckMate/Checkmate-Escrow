import { MatchStatusBadge } from './MatchStatusBadge';

interface MatchCardProps {
  matchId: number;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  status: 'pending' | 'active' | 'completed' | 'cancelled';
  platform: 'lichess' | 'chessdotcom';
}

export function MatchCard({ matchId, player1, player2, stakeAmount, token, status, platform }: MatchCardProps) {
  return (
    <div>
      <div>
        <span>Match #{matchId}</span>
        <MatchStatusBadge status={status} />
      </div>
      <dl>
        <dt>Player 1</dt><dd>{player1}</dd>
        <dt>Player 2</dt><dd>{player2}</dd>
        <dt>Stake</dt><dd>{stakeAmount} {token}</dd>
        <dt>Platform</dt><dd>{platform === 'lichess' ? 'Lichess' : 'Chess.com'}</dd>
      </dl>
    </div>
  );
}
