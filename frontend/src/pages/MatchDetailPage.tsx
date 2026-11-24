import { MatchDetail, type MatchDetailProps } from '../components/match/MatchDetail';

export interface MatchDetailPageProps {
  matchId: number;
  disputeStatus?: MatchDetailProps['disputeStatus'];
  player1?: string;
  player2?: string;
  stakeAmount?: string;
  token?: string;
  status?: MatchDetailProps['status'];
  platform?: MatchDetailProps['platform'];
  tokenDecimals?: number;
}

/**
 * Route target for `/match/:matchId`.
 *
 * This app does not depend on a routing library; App.tsx matches the path
 * with a simple regex and renders this page directly with the parsed id.
 * Real match data would be fetched by matchId here (e.g. via the indexer's
 * `/v1/indexer/match/{match_id}` endpoint) - wired up separately from the
 * deep-linking work this page exists for.
 */
export function MatchDetailPage({
  matchId,
  disputeStatus,
  player1 = '—',
  player2 = '—',
  stakeAmount = '—',
  token = '',
  status = 'pending',
  platform = 'lichess',
  tokenDecimals,
}: MatchDetailPageProps) {
  return (
    <main id="match-detail-page">
      <MatchDetail
        matchId={matchId}
        player1={player1}
        player2={player2}
        stakeAmount={stakeAmount}
        token={token}
        status={status}
        platform={platform}
        tokenDecimals={tokenDecimals}
        disputeStatus={disputeStatus}
      />
    </main>
  );
}
