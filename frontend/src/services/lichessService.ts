const LICHESS_API_BASE = import.meta.env.VITE_LICHESS_API_URL ?? 'https://lichess.org';

export interface LichessGame {
  id: string;
  opponent: string;
  result: string;
  createdAt: number;
  speed: string;
  variant: string;
}

interface LichessApiPlayer {
  user?: { name?: string };
}

interface LichessApiGame {
  id: string;
  createdAt: number;
  speed?: string;
  variant?: { key?: string };
  winner?: string;
  players: {
    white: LichessApiPlayer;
    black: LichessApiPlayer;
  };
}

/**
 * Fetch a player's recent games from the Lichess API.
 * Lichess returns newline-delimited JSON (ndjson) for this endpoint.
 */
export async function fetchRecentLichessGames(
  username: string,
  max = 20,
): Promise<LichessGame[]> {
  const trimmed = username.trim();
  if (!trimmed) return [];

  const url = `${LICHESS_API_BASE}/api/games/user/${encodeURIComponent(trimmed)}?max=${max}&pgnInJson=false`;

  const response = await fetch(url, {
    headers: { Accept: 'application/x-ndjson' },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch Lichess games: ${response.statusText}`);
  }

  const text = await response.text();
  if (!text.trim()) return [];

  return text
    .trim()
    .split('\n')
    .filter(Boolean)
    .map(line => JSON.parse(line) as LichessApiGame)
    .map(game => toLichessGame(game, trimmed));
}

function toLichessGame(game: LichessApiGame, username: string): LichessGame {
  const isWhite = game.players.white.user?.name?.toLowerCase() === username.toLowerCase();
  const opponent = isWhite
    ? game.players.black.user?.name ?? 'Unknown'
    : game.players.white.user?.name ?? 'Unknown';

  let result = 'draw';
  if (game.winner) {
    const won = (game.winner === 'white' && isWhite) || (game.winner === 'black' && !isWhite);
    result = won ? 'win' : 'loss';
  }

  return {
    id: game.id,
    opponent,
    result,
    createdAt: game.createdAt,
    speed: game.speed ?? 'unknown',
    variant: game.variant?.key ?? 'standard',
  };
}

export function formatGameLabel(game: LichessGame): string {
  const date = new Date(game.createdAt).toLocaleDateString();
  return `${game.id} · vs ${game.opponent} · ${game.result} · ${date}`;
}
