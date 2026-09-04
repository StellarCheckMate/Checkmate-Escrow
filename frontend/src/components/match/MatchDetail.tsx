import { useState } from 'react';
import { MatchStatusBadge } from './MatchStatusBadge';
import { formatTokenAmount } from '../../utils/tokenFormat';
import type { MatchReceiptProps } from './MatchReceiptPDF';

export interface MatchDetailProps {
  matchId: number;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  status: 'pending' | 'active' | 'completed' | 'cancelled';
  platform: 'lichess' | 'chessdotcom';
  /** Decimal places for `token`; when set, stakeAmount is formatted from raw units. */
  tokenDecimals?: number;
  /**
   * Receipt data required to generate a PDF payout receipt.
   * Only relevant (and shown) when `status === 'completed'`.
   */
  receipt?: Omit<MatchReceiptProps, 'matchId'>;
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
  tokenDecimals,
  receipt,
}: MatchDetailProps) {
  const [copied, setCopied] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const displayStake =
    tokenDecimals !== undefined ? formatTokenAmount(stakeAmount, tokenDecimals) : stakeAmount;

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

  const handleDownloadReceipt = async () => {
    if (!receipt) return;
    setDownloading(true);
    try {
      // Lazy-load the PDF module so it does not inflate the initial bundle.
      const { downloadMatchReceipt } = await import('./MatchReceiptPDF');
      await downloadMatchReceipt({ matchId, ...receipt });
    } finally {
      setDownloading(false);
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
        <dt>Stake</dt><dd>{displayStake} {token}</dd>
        <dt>Platform</dt><dd>{platform === 'lichess' ? 'Lichess' : 'Chess.com'}</dd>
      </dl>
      <button type="button" onClick={handleCopyLink}>
        {copied ? 'Link copied!' : 'Copy Link'}
      </button>
      {status === 'completed' && receipt && (
        <button
          type="button"
          onClick={handleDownloadReceipt}
          disabled={downloading}
          aria-label="Download payout receipt as PDF"
        >
          {downloading ? 'Generating…' : 'Download Receipt'}
        </button>
      )}
    </section>
  );
}
