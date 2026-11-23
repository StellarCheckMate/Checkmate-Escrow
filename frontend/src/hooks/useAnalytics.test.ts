import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useAnalytics } from './useAnalytics';

function feed(account: string, id: number) {
  return Response.json({
    success: true,
    data: [{ match_id: id, player1: account, player2: 'opponent', status: 'completed' }],
    error: null,
  });
}

afterEach(() => vi.unstubAllGlobals());

it('clears the previous overview and refetches when the account changes', async () => {
  let finishNextRequest!: (response: Response) => void;
  const nextResponse = new Promise<Response>(resolve => { finishNextRequest = resolve; });
  const fetchMatches = vi.fn()
    .mockResolvedValueOnce(feed('first', 1))
    .mockReturnValueOnce(nextResponse);
  vi.stubGlobal('fetch', fetchMatches);

  const { result, rerender } = renderHook(({ account }) => useAnalytics(account), {
    initialProps: { account: 'first' },
  });
  await waitFor(() => expect(result.current.result?.filteredMatches[0]?.match_id).toBe(1));

  rerender({ account: 'second' });
  expect(fetchMatches).toHaveBeenCalledTimes(2);
  expect(result.current.loading).toBe(true);
  expect(result.current.result).toBeNull();

  await act(async () => finishNextRequest(feed('second', 2)));
  await waitFor(() => expect(result.current.result?.filteredMatches[0]?.match_id).toBe(2));
});
