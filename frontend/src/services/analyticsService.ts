import type { MatchSummary } from '../hooks/useMatches';

export type TimePeriod = '7d' | '30d' | 'all';

export interface AnalyticsStats {
  totalEarnings: number;
  winRate: number;       // 0–100
  avgStake: number;
  totalMatches: number;
  wins: number;
  losses: number;
  draws: number;
}

export interface EarningsPoint {
  date: string;          // YYYY-MM-DD
  earnings: number;
}

export interface WinRatePoint {
  date: string;
  winRate: number;       // 0–100
}

export interface AnalyticsResult {
  stats: AnalyticsStats;
  earningsSeries: EarningsPoint[];
  winRateSeries: WinRatePoint[];
  filteredMatches: MatchSummary[];
}

function cutoffDate(period: TimePeriod): Date | null {
  if (period === 'all') return null;
  const d = new Date();
  d.setDate(d.getDate() - (period === '7d' ? 7 : 30));
  d.setHours(0, 0, 0, 0);
  return d;
}

/** Filter matches to those involving `userAddress` within the time period. */
export function filterMatches(
  matches: MatchSummary[],
  userAddress: string,
  period: TimePeriod,
  tokenFilter?: string,
  statusFilter?: string,
): MatchSummary[] {
  const cutoff = cutoffDate(period);
  return matches.filter(m => {
    if (m.player1 !== userAddress && m.player2 !== userAddress) return false;
    if (tokenFilter && m.token !== tokenFilter) return false;
    if (statusFilter && m.status !== statusFilter) return false;
    if (cutoff && m.timestamp) {
      const ts = new Date(m.timestamp);
      if (ts < cutoff) return false;
    }
    return true;
  });
}

/** Calculate aggregate stats. Earnings = stake won minus stake lost. */
export function calcStats(matches: MatchSummary[], userAddress: string): AnalyticsStats {
  let wins = 0, losses = 0, draws = 0;
  let totalEarnings = 0;
  let stakeSum = 0;
  const completed = matches.filter(m => m.status === 'completed');

  for (const m of completed) {
    const stake = parseFloat(m.stake_amount ?? '0') || 0;
    stakeSum += stake;
    if (!m.winner) {
      // draw — stake returned
      draws++;
    } else if (m.winner === userAddress) {
      wins++;
      totalEarnings += stake; // won opponent's stake
    } else {
      losses++;
      totalEarnings -= stake; // lost own stake
    }
  }

  const totalMatches = completed.length;
  const winRate = totalMatches > 0 ? (wins / totalMatches) * 100 : 0;
  const avgStake = totalMatches > 0 ? stakeSum / totalMatches : 0;

  return { totalEarnings, winRate, avgStake, totalMatches, wins, losses, draws };
}

/** Build cumulative earnings time series grouped by day. */
export function buildEarningsSeries(matches: MatchSummary[], userAddress: string): EarningsPoint[] {
  const byDay = new Map<string, number>();

  const completed = matches.filter(m => m.status === 'completed' && m.timestamp);
  completed.sort((a, b) => new Date(a.timestamp!).getTime() - new Date(b.timestamp!).getTime());

  for (const m of completed) {
    const date = m.timestamp!.slice(0, 10);
    const stake = parseFloat(m.stake_amount ?? '0') || 0;
    let delta = 0;
    if (!m.winner) {
      delta = 0;
    } else if (m.winner === userAddress) {
      delta = stake;
    } else {
      delta = -stake;
    }
    byDay.set(date, (byDay.get(date) ?? 0) + delta);
  }

  let cumulative = 0;
  return Array.from(byDay.entries()).map(([date, delta]) => {
    cumulative += delta;
    return { date, earnings: cumulative };
  });
}

/** Build win-rate time series: rolling win-rate up to each day. */
export function buildWinRateSeries(matches: MatchSummary[], userAddress: string): WinRatePoint[] {
  const completed = matches.filter(m => m.status === 'completed' && m.timestamp);
  completed.sort((a, b) => new Date(a.timestamp!).getTime() - new Date(b.timestamp!).getTime());

  let wins = 0, total = 0;
  const seen = new Map<string, { wins: number; total: number }>();

  for (const m of completed) {
    const date = m.timestamp!.slice(0, 10);
    total++;
    if (m.winner === userAddress) wins++;
    seen.set(date, { wins, total });
  }

  return Array.from(seen.entries()).map(([date, { wins: w, total: t }]) => ({
    date,
    winRate: t > 0 ? Math.round((w / t) * 100) : 0,
  }));
}

export function computeAnalytics(
  matches: MatchSummary[],
  userAddress: string,
  period: TimePeriod,
  tokenFilter?: string,
  statusFilter?: string,
): AnalyticsResult {
  const filteredMatches = filterMatches(matches, userAddress, period, tokenFilter, statusFilter);
  return {
    stats: calcStats(filteredMatches, userAddress),
    earningsSeries: buildEarningsSeries(filteredMatches, userAddress),
    winRateSeries: buildWinRateSeries(filteredMatches, userAddress),
    filteredMatches,
  };
}

export function exportToCsv(matches: MatchSummary[], filename = 'match-history.csv'): void {
  const headers = ['match_id', 'status', 'player1', 'player2', 'winner', 'stake_amount', 'token', 'platform', 'timestamp'];
  const rows = matches.map(m =>
    headers.map(h => {
      const val = (m as Record<string, unknown>)[h] ?? '';
      const s = String(val);
      return s.includes(',') || s.includes('"') ? `"${s.replace(/"/g, '""')}"` : s;
    }).join(',')
  );
  const csv = [headers.join(','), ...rows].join('\n');
  const blob = new Blob([csv], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
