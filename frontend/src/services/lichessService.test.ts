import { vi } from 'vitest';
import { fetchRecentLichessGames, formatGameLabel, InvalidLichessTokenError } from './lichessService';

const NDJSON_RESPONSE = [
  JSON.stringify({
    id: 'abcd1234',
    createdAt: 1700000000000,
    speed: 'blitz',
    variant: { key: 'standard' },
    winner: 'white',
    players: {
      white: { user: { name: 'TestPlayer' } },
      black: { user: { name: 'Opponent1' } },
    },
  }),
  JSON.stringify({
    id: 'efgh5678',
    createdAt: 1700003600000,
    speed: 'rapid',
    variant: { key: 'standard' },
    players: {
      white: { user: { name: 'Opponent2' } },
      black: { user: { name: 'TestPlayer' } },
    },
  }),
].join('\n');

describe('lichessService', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  test('fetches and normalizes recent games for a username', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(NDJSON_RESPONSE, { status: 200 })));

    const games = await fetchRecentLichessGames('TestPlayer');

    expect(games).toHaveLength(2);
    expect(games[0]).toMatchObject({ id: 'abcd1234', opponent: 'Opponent1', result: 'win' });
    expect(games[1]).toMatchObject({ id: 'efgh5678', opponent: 'Opponent2', result: 'draw' });
  });

  test('returns an empty array for a blank username', async () => {
    vi.stubGlobal('fetch', vi.fn());
    const games = await fetchRecentLichessGames('   ');
    expect(games).toEqual([]);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  test('throws InvalidLichessTokenError when the Lichess API responds with a 401', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 401,
      statusText: 'Unauthorized',
    }) as unknown as typeof fetch;

    await expect(fetchRecentLichessGames('TestPlayer')).rejects.toThrow(InvalidLichessTokenError);
    await expect(fetchRecentLichessGames('TestPlayer')).rejects.toThrow('Invalid Lichess API token');
  });

  test('throws when the Lichess API responds with an error', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
    }) as unknown as typeof fetch;

    await expect(fetchRecentLichessGames('missinguser')).rejects.toThrow('Not Found');
  });

  test('exposes a typed HTTP error on 401', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 401, statusText: 'Unauthorized' })));
    const request = fetchRecentLichessGames('TestPlayer');
    await expect(request).rejects.toBeInstanceOf(LichessHttpError);
    await expect(request).rejects.toMatchObject({ status: 401, message: expect.stringContaining('Unauthorized') });
  });

  test('formatGameLabel includes id, opponent, and result', () => {
    const label = formatGameLabel({
      id: 'abcd1234',
      opponent: 'Opponent1',
      result: 'win',
      createdAt: 1700000000000,
      speed: 'blitz',
      variant: 'standard',
    });
    expect(label).toContain('abcd1234');
    expect(label).toContain('Opponent1');
    expect(label).toContain('win');
  });
});
