import { useState } from 'react';
import { MatchStatusBadge } from './MatchStatusBadge';

export interface MatchDetailProps {
  matchId: number;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  status: 'pending' | 'active' | 'completed' | 'cancelled';
  platform: 'lichess' | 'chessdotcom';
}

/**
 * Builds the shareable deep-link URL for a given match, e.g. `/match/1234`.
 * Exported for reuse/testing outside of the component's click handler.
 */
export function buildMatchLink(matchId: number, origin: string = window.location.origin): string {
  return `${origin}/match/${matchId}`;
}

export function MatchDetail({
  matchId,
  player1,
  player2,
  stakeAmount,
  token,
  status,
  platform,
}: MatchDetailProps) {
  const [copied, setCopied] = useState(false);

  const handleCopyLink = async () => {
    const link = buildMatchLink(matchId);
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API unavailable (e.g. insecure context) - fail silently,
      // the button remains usable for a retry.
    }
  };

  return (
    <section aria-label={`Match ${matchId} details`}>
      <header>
        <h2>Match #{matchId}</h2>
        <MatchStatusBadge status={status} />
      </header>
      <dl>
        <dt>Player 1</dt><dd>{player1}</dd>
        <dt>Player 2</dt><dd>{player2}</dd>
        <dt>Stake</dt><dd>{stakeAmount} {token}</dd>
        <dt>Platform</dt><dd>{platform === 'lichess' ? 'Lichess' : 'Chess.com'}</dd>
      </dl>
      <button type="button" onClick={handleCopyLink}>
        {copied ? 'Link copied!' : 'Copy Link'}
      </button>
    </section>
  );
}
