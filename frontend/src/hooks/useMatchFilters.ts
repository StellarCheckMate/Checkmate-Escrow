import { useState, useMemo, useCallback, useEffect, useRef } from 'react';
import { useMatches } from './useMatches';
import type { MatchSummary } from './useMatches';
import { DEFAULT_FILTERS } from './filterTypes';
import type { MatchFilters, MatchStatus } from './filterTypes';
import { applyFilters, extractTokens } from './matchFilterService';

// Preset filters
export const PRESETS: Record<string, Partial<MatchFilters>> = {
  'All Matches': DEFAULT_FILTERS,
  'Active': { statuses: ['active'] },
  'Pending': { statuses: ['pending'] },
  'Completed': { statuses: ['completed'] },
};

const VALID_STATUSES: Set<MatchStatus> = new Set([
  'pending',
  'active',
  'completed',
  'cancelled',
  'expired',
]);

/**
 * Parse filter parameters from a URL search string or URLSearchParams.
 */
export function parseFiltersFromSearchParams(searchParams: URLSearchParams): MatchFilters {
  const statusesParam = searchParams.get('status') ?? searchParams.get('statuses');
  const statuses: MatchStatus[] = statusesParam
    ? (statusesParam
        .split(',')
        .map(s => s.trim())
        .filter(s => VALID_STATUSES.has(s as MatchStatus)) as MatchStatus[])
    : [];

  const tokensParam = searchParams.get('token') ?? searchParams.get('tokens');
  const tokens = tokensParam
    ? tokensParam
        .split(',')
        .map(t => t.trim())
        .filter(Boolean)
    : [];

  const playerQuery = searchParams.get('player') ?? searchParams.get('playerQuery') ?? '';

  const stakeMinParam = searchParams.get('stakeMin');
  const stakeMin = stakeMinParam !== null && !isNaN(Number(stakeMinParam)) && stakeMinParam !== ''
    ? Number(stakeMinParam)
    : null;

  const stakeMaxParam = searchParams.get('stakeMax');
  const stakeMax = stakeMaxParam !== null && !isNaN(Number(stakeMaxParam)) && stakeMaxParam !== ''
    ? Number(stakeMaxParam)
    : null;

  const dateFrom = searchParams.get('dateFrom') || null;
  const dateTo = searchParams.get('dateTo') || null;

  return {
    statuses,
    tokens,
    playerQuery,
    stakeMin,
    stakeMax,
    dateFrom,
    dateTo,
  };
}

/**
 * Convert MatchFilters into URLSearchParams.
 */
export function filtersToSearchParams(filters: MatchFilters): URLSearchParams {
  const params = new URLSearchParams();

  if (filters.statuses && filters.statuses.length > 0) {
    params.set('status', filters.statuses.join(','));
  }
  if (filters.tokens && filters.tokens.length > 0) {
    params.set('token', filters.tokens.join(','));
  }
  if (filters.playerQuery && filters.playerQuery.trim() !== '') {
    params.set('player', filters.playerQuery.trim());
  }
  if (filters.stakeMin !== null && filters.stakeMin !== undefined) {
    params.set('stakeMin', String(filters.stakeMin));
  }
  if (filters.stakeMax !== null && filters.stakeMax !== undefined) {
    params.set('stakeMax', String(filters.stakeMax));
  }
  if (filters.dateFrom) {
    params.set('dateFrom', filters.dateFrom);
  }
  if (filters.dateTo) {
    params.set('dateTo', filters.dateTo);
  }

  return params;
}

/**
 * Check whether two MatchFilters are structurally equal.
 */
function areFiltersEqual(a: MatchFilters, b: MatchFilters): boolean {
  if (a.playerQuery !== b.playerQuery) return false;
  if (a.stakeMin !== b.stakeMin) return false;
  if (a.stakeMax !== b.stakeMax) return false;
  if (a.dateFrom !== b.dateFrom) return false;
  if (a.dateTo !== b.dateTo) return false;

  if (a.statuses.length !== b.statuses.length) return false;
  for (let i = 0; i < a.statuses.length; i++) {
    if (a.statuses[i] !== b.statuses[i]) return false;
  }

  if (a.tokens.length !== b.tokens.length) return false;
  for (let i = 0; i < a.tokens.length; i++) {
    if (a.tokens[i] !== b.tokens[i]) return false;
  }

  return true;
}

export function useMatchFilters(currentPlayer?: string) {
  const { matches: allMatches, loading, error } = useMatches({ limit: 1000 });

  const [filters, setFilters] = useState<MatchFilters>(() => {
    if (typeof window !== 'undefined' && window.location && window.location.search) {
      const parsed = parseFiltersFromSearchParams(new URLSearchParams(window.location.search));
      const hasAnyFilter =
        parsed.statuses.length > 0 ||
        parsed.tokens.length > 0 ||
        Boolean(parsed.playerQuery) ||
        parsed.stakeMin !== null ||
        parsed.stakeMax !== null ||
        Boolean(parsed.dateFrom) ||
        Boolean(parsed.dateTo);
      if (hasAnyFilter) {
        return parsed;
      }
    }
    return DEFAULT_FILTERS;
  });

  const lastSyncedQueryRef = useRef<string>('');

  // Sync state to URL query string whenever filters change
  useEffect(() => {
    if (typeof window === 'undefined' || !window.location) return;

    const currentUrl = new URL(window.location.href);
    const existingParams = currentUrl.searchParams;

    // Filter keys we manage
    const managedKeys = ['status', 'statuses', 'token', 'tokens', 'player', 'playerQuery', 'stakeMin', 'stakeMax', 'dateFrom', 'dateTo'];
    for (const key of managedKeys) {
      existingParams.delete(key);
    }

    const filterParams = filtersToSearchParams(filters);
    filterParams.forEach((value, key) => {
      existingParams.set(key, value);
    });

    const newQuery = existingParams.toString();
    const newSearch = newQuery ? `?${newQuery}` : '';
    const newRelativeUrl = `${currentUrl.pathname}${newSearch}${currentUrl.hash}`;

    if (window.location.search !== newSearch) {
      lastSyncedQueryRef.current = newSearch;
      window.history.replaceState(window.history.state, '', newRelativeUrl);
    }
  }, [filters]);

  // Listen to popstate events (e.g. browser back/forward or external navigation)
  useEffect(() => {
    if (typeof window === 'undefined' || !window.addEventListener) return;

    const handlePopState = () => {
      const currentSearch = window.location.search;
      if (currentSearch === lastSyncedQueryRef.current) return;
      const parsed = parseFiltersFromSearchParams(new URLSearchParams(currentSearch));
      setFilters(prev => (areFiltersEqual(prev, parsed) ? prev : parsed));
    };

    window.addEventListener('popstate', handlePopState);
    return () => {
      window.removeEventListener('popstate', handlePopState);
    };
  }, []);

  const updateFilter = useCallback(<K extends keyof MatchFilters>(key: K, value: MatchFilters[K]) => {
    setFilters(prev => ({ ...prev, [key]: value }));
  }, []);

  const applyPreset = useCallback((name: string) => {
    setFilters({ ...DEFAULT_FILTERS, ...(PRESETS[name] ?? {}) });
  }, []);

  const myMatchesFilter = useCallback(() => {
    if (!currentPlayer) return;
    setFilters(prev => ({ ...prev, playerQuery: currentPlayer }));
  }, [currentPlayer]);

  const reset = useCallback(() => setFilters(DEFAULT_FILTERS), []);

  const filtered: MatchSummary[] = useMemo(
    () => applyFilters(allMatches, filters),
    [allMatches, filters]
  );

  const availableTokens = useMemo(() => extractTokens(allMatches), [allMatches]);

  return { filters, filtered, availableTokens, loading, error, updateFilter, applyPreset, myMatchesFilter, reset };
}
