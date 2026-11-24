import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LichessGamePicker } from './LichessGamePicker';
import * as lichessService from '../../services/lichessService';

const MOCK_GAMES: lichessService.LichessGame[] = [
  {
    id: 'abcd1234',
    opponent: 'Opponent1',
    result: 'win',
    createdAt: 1700000000000,
    speed: 'blitz',
    variant: 'standard',
  },
  {
    id: 'efgh5678',
    opponent: 'Opponent2',
    result: 'loss',
    createdAt: 1700003600000,
    speed: 'rapid',
    variant: 'standard',
  },
];

describe('LichessGamePicker', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('loads games for a username and allows selecting a game once loaded', async () => {
    vi.spyOn(lichessService, 'fetchRecentLichessGames').mockResolvedValue(MOCK_GAMES);
    const onSelect = vi.fn();

    render(<LichessGamePicker onSelect={onSelect} />);

    fireEvent.change(screen.getByLabelText(/lichess username/i), {
      target: { value: 'DrNykterstein' },
    });

    const firstButton = await screen.findByRole('button', { name: /abcd1234/i });
    expect(firstButton).not.toBeDisabled();

    fireEvent.click(firstButton);
    expect(onSelect).toHaveBeenCalledWith('abcd1234');
  });

  it('disables game selection and search while games are loading', async () => {
    let resolveNextFetch!: (games: lichessService.LichessGame[]) => void;
    const fetchSpy = vi
      .spyOn(lichessService, 'fetchRecentLichessGames')
      .mockResolvedValueOnce(MOCK_GAMES)
      .mockImplementationOnce(
        () =>
          new Promise<lichessService.LichessGame[]>(resolve => {
            resolveNextFetch = resolve;
          }),
      );

    const onSelect = vi.fn();
    render(<LichessGamePicker onSelect={onSelect} />);

    // Initial load populates the game picker
    fireEvent.change(screen.getByLabelText(/lichess username/i), {
      target: { value: 'PlayerOne' },
    });

    const firstGameBtn = await screen.findByRole('button', { name: /abcd1234/i });
    expect(firstGameBtn).not.toBeDisabled();

    // Changing the username triggers a new in-flight query while existing games remain listed
    fireEvent.change(screen.getByLabelText(/lichess username/i), {
      target: { value: 'PlayerTwo' },
    });

    expect(screen.getByRole('status')).toHaveTextContent(/loading recent games/i);
    expect(fetchSpy).toHaveBeenCalledTimes(2);

    const searchInput = screen.getByLabelText(/search games/i);
    expect(searchInput).toBeDisabled();

    const listbox = screen.getByRole('listbox', { name: /recent lichess games/i });
    expect(listbox).toHaveAttribute('aria-disabled', 'true');

    const gameButtons = screen.getAllByRole('button');
    expect(gameButtons.length).toBeGreaterThan(0);
    for (const btn of gameButtons) {
      expect(btn).toBeDisabled();
      fireEvent.click(btn);
    }
    expect(onSelect).not.toHaveBeenCalled();

    // Resolve the second query and verify selection is re-enabled
    resolveNextFetch(MOCK_GAMES);
    await waitFor(() => {
      expect(screen.queryByRole('status')).toBeNull();
    });

    const enabledBtn = screen.getByRole('button', { name: /abcd1234/i });
    expect(enabledBtn).not.toBeDisabled();
    fireEvent.click(enabledBtn);
    expect(onSelect).toHaveBeenCalledWith('abcd1234');
  });
});
