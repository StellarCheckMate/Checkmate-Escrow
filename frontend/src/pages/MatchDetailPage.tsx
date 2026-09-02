import { MatchDetail } from '../components/match/MatchDetail';

/**
 * Route target for `/match/:matchId`.
 *
 * This app does not depend on a routing library; App.tsx matches the path
 * with a simple regex and renders this page directly with the parsed id.
 * Real match data would be fetched by matchId here (e.g. via the indexer's
 * `/v1/indexer/match/{match_id}` endpoint) - wired up separately from the
 * deep-linking work this page exists for.
 */
export function MatchDetailPage({ matchId }: { matchId: number }) {
  return (
    <main id="match-detail-page">
      <MatchDetail
        matchId={matchId}
        player1="—"
        player2="—"
        stakeAmount="—"
        token=""
        status="pending"
        platform="lichess"
      />
    </main>
  );
}
