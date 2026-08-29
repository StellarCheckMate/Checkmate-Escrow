import { useEffect, useMemo, useState } from 'react';
import { fetchRecentLichessGames, formatGameLabel, type LichessGame } from '../../services/lichessService';

interface LichessGamePickerProps {
  onSelect: (gameId: string) => void;
}

export function LichessGamePicker({ onSelect }: LichessGamePickerProps) {
  const [username, setUsername] = useState('');
  const [games, setGames] = useState<LichessGame[]>([]);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!username.trim()) {
      setGames([]);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    fetchRecentLichessGames(username)
      .then(result => {
        if (!cancelled) setGames(result);
      })
      .catch(err => {
        if (!cancelled) setError((err as Error).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [username]);

  const filteredGames = useMemo(() => {
    if (!query.trim()) return games;
    const q = query.toLowerCase();
    return games.filter(
      g => g.id.toLowerCase().includes(q) || g.opponent.toLowerCase().includes(q),
    );
  }, [games, query]);

  return (
    <div className="lichess-game-picker">
      <label htmlFor="lichess-username">Lichess Username</label>
      <input
        id="lichess-username"
        value={username}
        onChange={ev => setUsername(ev.target.value)}
        placeholder="e.g. DrNykterstein"
      />

      {loading && <p role="status">Loading recent games…</p>}
      {error && <p role="alert">{error}</p>}

      {!loading && !error && games.length > 0 && (
        <>
          <label htmlFor="lichess-game-search">Search Games</label>
          <input
            id="lichess-game-search"
            value={query}
            onChange={ev => setQuery(ev.target.value)}
            placeholder="Search by game ID or opponent"
          />
          <ul aria-label="Recent Lichess games" role="listbox">
            {filteredGames.map(game => (
              <li key={game.id}>
                <button type="button" onClick={() => onSelect(game.id)}>
                  {formatGameLabel(game)}
                </button>
              </li>
            ))}
          </ul>
          {filteredGames.length === 0 && <p>No games match your search.</p>}
        </>
      )}
    </div>
  );
}
