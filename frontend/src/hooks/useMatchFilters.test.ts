import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import {
  useMatchFilters,
  parseFiltersFromSearchParams,
  filtersToSearchParams,
  PRESETS,
} from './useMatchFilters';
import * as matchesHook from './useMatches';
import type { MatchSummary } from './useMatches';
import { DEFAULT_FILTERS } from './filterTypes';

vi.mock('./useMatches');

describe('useMatchFilters URL synchronization', () => {
  const mockMatches: MatchSummary[] = [
    {
      match_id: 1,
      player1: 'GACCOUNT_A',
      player2: 'GACCOUNT_B',
      status: 'active',
      stake_amount: '100',
      token: 'XLM',
      timestamp: '2026-01-01T10:00:00Z',
    },
    {
      match_id: 2,
      player1: 'GACCOUNT_C',
      player2: 'GACCOUNT_D',
      status: 'pending',
      stake_amount: '500',
      token: 'USDC',
      timestamp: '2026-01-02T10:00:00Z',
    },
    {
      match_id: 3,
      player1: 'GACCOUNT_A',
      player2: 'GACCOUNT_C',
      status: 'completed',
      stake_amount: '50',
      token: 'XLM',
      timestamp: '2026-01-03T10:00:00Z',
    },
  ];

  const originalLocation = window.location;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(matchesHook.useMatches).mockReturnValue({
      matches: mockMatches,
      loading: false,
      error: null,
      total: mockMatches.length,
    });

    // Reset window.location to a clean state
    delete (window as unknown as { location?: unknown }).location;
    window.location = new URL('https://checkmate.example/matches') as unknown as Location;
    window.history.replaceState({}, '', '/matches');
  });

  afterEach(() => {
    window.location = originalLocation;
  });

  it('parses filters correctly from search params', () => {
    const params = new URLSearchParams(
      'status=active,pending&token=XLM,USDC&player=GACCOUNT_A&stakeMin=50&stakeMax=500&dateFrom=2026-01-01&dateTo=2026-01-31'
    );
    const filters = parseFiltersFromSearchParams(params);

    expect(filters.statuses).toEqual(['active', 'pending']);
    expect(filters.tokens).toEqual(['XLM', 'USDC']);
    expect(filters.playerQuery).toBe('GACCOUNT_A');
    expect(filters.stakeMin).toBe(50);
    expect(filters.stakeMax).toBe(500);
    expect(filters.dateFrom).toBe('2026-01-01');
    expect(filters.dateTo).toBe('2026-01-31');
  });

  it('initializes filters from URL query string on mount', () => {
    window.location = new URL(
      'https://checkmate.example/matches?status=active&token=XLM&player=GACCOUNT_A&stakeMin=10&stakeMax=200'
    ) as unknown as Location;

    const { result } = renderHook(() => useMatchFilters());

    expect(result.current.filters.statuses).toEqual(['active']);
    expect(result.current.filters.tokens).toEqual(['XLM']);
    expect(result.current.filters.playerQuery).toBe('GACCOUNT_A');
    expect(result.current.filters.stakeMin).toBe(10);
    expect(result.current.filters.stakeMax).toBe(200);

    // Filtered matches should reflect active filters from URL
    expect(result.current.filtered).toHaveLength(1);
    expect(result.current.filtered[0].match_id).toBe(1);
  });

  it('updates the URL query string when a filter is updated', () => {
    const replaceStateSpy = vi.spyOn(window.history, 'replaceState');
    const { result } = renderHook(() => useMatchFilters());

    expect(result.current.filters).toEqual(DEFAULT_FILTERS);

    act(() => {
      result.current.updateFilter('statuses', ['pending']);
    });

    expect(result.current.filters.statuses).toEqual(['pending']);
    expect(window.location.search).toContain('status=pending');
    expect(replaceStateSpy).toHaveBeenCalled();
  });

  it('updates the URL query string when preset is applied', () => {
    const { result } = renderHook(() => useMatchFilters());

    act(() => {
      result.current.applyPreset('Active');
    });

    expect(result.current.filters.statuses).toEqual(['active']);
    const params = new URLSearchParams(window.location.search);
    expect(params.get('status')).toBe('active');
  });

  it('updates the URL query string when myMatchesFilter is applied', () => {
    const { result } = renderHook(() => useMatchFilters('GACCOUNT_A'));

    act(() => {
      result.current.myMatchesFilter();
    });

    expect(result.current.filters.playerQuery).toBe('GACCOUNT_A');
    const params = new URLSearchParams(window.location.search);
    expect(params.get('player')).toBe('GACCOUNT_A');
  });

  it('clears URL query parameters on reset', () => {
    window.location = new URL(
      'https://checkmate.example/matches?status=completed&player=GACCOUNT_C'
    ) as unknown as Location;

    const { result } = renderHook(() => useMatchFilters());
    expect(result.current.filters.statuses).toEqual(['completed']);

    act(() => {
      result.current.reset();
    });

    expect(result.current.filters).toEqual(DEFAULT_FILTERS);
    expect(window.location.search).toBe('');
  });

  it('preserves unrelated search params when updating filters', () => {
    window.location = new URL(
      'https://checkmate.example/matches?theme=dark&admin=1'
    ) as unknown as Location;

    const { result } = renderHook(() => useMatchFilters());

    act(() => {
      result.current.updateFilter('playerQuery', 'alice');
    });

    const params = new URLSearchParams(window.location.search);
    expect(params.get('theme')).toBe('dark');
    expect(params.get('admin')).toBe('1');
    expect(params.get('player')).toBe('alice');
  });

  it('asserts filters round-trip through the URL', () => {
    // 1. Initial render with no query
    const { result, unmount } = renderHook(() => useMatchFilters());

    // 2. Set multiple filters
    act(() => {
      result.current.updateFilter('statuses', ['active', 'pending']);
      result.current.updateFilter('tokens', ['XLM']);
      result.current.updateFilter('playerQuery', 'GACCOUNT');
      result.current.updateFilter('stakeMin', 50);
      result.current.updateFilter('stakeMax', 1000);
      result.current.updateFilter('dateFrom', '2026-01-01');
      result.current.updateFilter('dateTo', '2026-01-31');
    });

    const savedSearch = window.location.search;
    expect(savedSearch).toBeTruthy();
    unmount();

    // 3. Re-render simulate page reload with the saved URL search string
    window.location = new URL(`https://checkmate.example/matches${savedSearch}`) as unknown as Location;
    const reloaded = renderHook(() => useMatchFilters());

    // 4. Assert round-trip state equality
    expect(reloaded.result.current.filters.statuses).toEqual(['active', 'pending']);
    expect(reloaded.result.current.filters.tokens).toEqual(['XLM']);
    expect(reloaded.result.current.filters.playerQuery).toBe('GACCOUNT');
    expect(reloaded.result.current.filters.stakeMin).toBe(50);
    expect(reloaded.result.current.filters.stakeMax).toBe(1000);
    expect(reloaded.result.current.filters.dateFrom).toBe('2026-01-01');
    expect(reloaded.result.current.filters.dateTo).toBe('2026-01-31');
  });

  it('syncs state on browser popstate navigation', () => {
    const { result } = renderHook(() => useMatchFilters());

    act(() => {
      window.location = new URL(
        'https://checkmate.example/matches?status=expired&stakeMin=25'
      ) as unknown as Location;
      window.dispatchEvent(new PopStateEvent('popstate'));
    });

    expect(result.current.filters.statuses).toEqual(['expired']);
    expect(result.current.filters.stakeMin).toBe(25);
  });
});
