import { useState, useEffect, useRef } from 'react';
import { useMatches } from './useMatches';
import { computeAnalytics } from '../services/analyticsService';
import type { TimePeriod, AnalyticsResult } from '../services/analyticsService';

const CACHE_TTL_MS = 5 * 60 * 1000;

interface CacheEntry {
  result: AnalyticsResult;
  ts: number;
}

const cache = new Map<string, CacheEntry>();

function cacheKey(userAddress: string, period: TimePeriod, token?: string, status?: string) {
  return `${userAddress}:${period}:${token ?? ''}:${status ?? ''}`;
}

export function useAnalytics(
  userAddress: string | null,
  period: TimePeriod = 'all',
  tokenFilter?: string,
  statusFilter?: string,
) {
  const { matches, loading: matchesLoading, error: matchesError } = useMatches({ limit: 500 });
  const [result, setResult] = useState<AnalyticsResult | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!userAddress || matchesLoading) return;

    const key = cacheKey(userAddress, period, tokenFilter, statusFilter);
    const cached = cache.get(key);
    if (cached && Date.now() - cached.ts < CACHE_TTL_MS) {
      setResult(cached.result);
      return;
    }

    const computed = computeAnalytics(matches, userAddress, period, tokenFilter, statusFilter);
    cache.set(key, { result: computed, ts: Date.now() });
    setResult(computed);

    // Invalidate cache after TTL so next render recomputes
    timerRef.current = setTimeout(() => cache.delete(key), CACHE_TTL_MS);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [matches, matchesLoading, userAddress, period, tokenFilter, statusFilter]);

  return {
    result,
    loading: matchesLoading,
    error: matchesError,
  };
}
