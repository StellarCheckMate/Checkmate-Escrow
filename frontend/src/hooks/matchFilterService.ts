import { MatchSummary } from './useMatches';
import { MatchFilters } from './filterTypes';

/** Apply all active filters to a list of matches. Pure function — O(n). */
export function applyFilters(matches: MatchSummary[], filters: MatchFilters): MatchSummary[] {
  return matches.filter(m => {
    if (filters.statuses.length && !filters.statuses.includes(m.status as never)) return false;

    if (filters.tokens.length && !filters.tokens.includes(m.token ?? '')) return false;

    if (filters.playerQuery) {
      const q = filters.playerQuery.toLowerCase();
      const hits = [m.player1, m.player2].some(p => p.toLowerCase().includes(q));
      if (!hits) return false;
    }

    if (m.stake_amount !== undefined) {
      const stake = Number(m.stake_amount);
      if (filters.stakeMin !== null && stake < filters.stakeMin) return false;
      if (filters.stakeMax !== null && stake > filters.stakeMax) return false;
    }

    if (m.timestamp) {
      const ts = new Date(m.timestamp).getTime();
      if (filters.dateFrom && ts < new Date(filters.dateFrom).getTime()) return false;
      if (filters.dateTo) {
        const end = new Date(filters.dateTo);
        end.setHours(23, 59, 59, 999);
        if (ts > end.getTime()) return false;
      }
    }

    return true;
  });
}

/** Derive the sorted unique token list from a match set. */
export function extractTokens(matches: MatchSummary[]): string[] {
  return [...new Set(matches.map(m => m.token).filter(Boolean) as string[])].sort();
}
