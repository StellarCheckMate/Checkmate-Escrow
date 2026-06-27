import { useState, useMemo, useCallback } from 'react';
import { useMatches, MatchSummary } from './useMatches';
import { MatchFilters, DEFAULT_FILTERS } from './filterTypes';
import { applyFilters, extractTokens } from './matchFilterService';

// Preset filters
export const PRESETS: Record<string, Partial<MatchFilters>> = {
  'All Matches': DEFAULT_FILTERS,
  'Active': { statuses: ['active'] },
  'Pending': { statuses: ['pending'] },
  'Completed': { statuses: ['completed'] },
};

export function useMatchFilters(currentPlayer?: string) {
  const { matches: allMatches, loading, error } = useMatches({ limit: 1000 });

  const [filters, setFilters] = useState<MatchFilters>(DEFAULT_FILTERS);

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
