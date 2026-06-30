export type MatchStatus = 'pending' | 'active' | 'completed' | 'cancelled' | 'expired';

export interface MatchFilters {
  statuses: MatchStatus[];   // empty = all
  tokens: string[];          // empty = all
  playerQuery: string;       // address substring or username
  stakeMin: number | null;   // stroops (null = unbounded)
  stakeMax: number | null;
  dateFrom: string | null;   // ISO date string
  dateTo: string | null;
}

export const DEFAULT_FILTERS: MatchFilters = {
  statuses: [],
  tokens: [],
  playerQuery: '',
  stakeMin: null,
  stakeMax: null,
  dateFrom: null,
  dateTo: null,
};
