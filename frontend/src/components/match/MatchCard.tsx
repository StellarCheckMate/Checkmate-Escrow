import { MatchStatusBadge } from './MatchStatusBadge';
import { formatTokenAmount } from '../../utils/tokenFormat';

interface MatchCardProps {
  matchId: number;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  status: 'pending' | 'active' | 'completed' | 'cancelled';
  platform: 'lichess' | 'chessdotcom';
  /**
   * Decimal places for `token` (e.g. 7 for most Stellar assets). When
   * provided, `stakeAmount` is treated as a raw integer (stroops) and
   * formatted to a human-readable decimal string. When omitted,
   * `stakeAmount` is rendered as-is for backward compatibility.
   */
  tokenDecimals?: number;
}

export function MatchCard({ matchId, player1, player2, stakeAmount, token, status, platform, tokenDecimals }: MatchCardProps) {
  const displayStake =
    tokenDecimals !== undefined ? formatTokenAmount(stakeAmount, tokenDecimals) : stakeAmount;

  return (
    <div>
      <div>
        <span>Match #{matchId}</span>
        <MatchStatusBadge status={status} />
      </div>
      <dl>
        <dt>Player 1</dt><dd>{player1}</dd>
        <dt>Player 2</dt><dd>{player2}</dd>
        <dt>Stake</dt><dd>{displayStake} {token}</dd>
        <dt>Platform</dt><dd>{platform === 'lichess' ? 'Lichess' : 'Chess.com'}</dd>
      </dl>
    </div>
  );
}
