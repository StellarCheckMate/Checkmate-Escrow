import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MatchList } from '../components/MatchList';

const baseMatch = {
  matchId: 1,
  player1: 'GAAA',
  player2: 'GBBB',
  stakeAmount: '50',
  token: 'USDC',
  status: 'active' as const,
  platform: 'lichess' as const,
};

describe('MatchList', () => {
  it('shows loading state', () => {
    render(<MatchList matches={[]} loading />);
    expect(screen.getByRole('status').textContent).toBe('Loading matches…');
  });

  it('shows error state', () => {
    render(<MatchList matches={[]} error="Failed to load" />);
    expect(screen.getByRole('alert').textContent).toBe('Failed to load');
  });

  it('shows empty state when no matches', () => {
    render(<MatchList matches={[]} />);
    expect(screen.getByText('No matches found.')).toBeTruthy();
  });

  it('renders a list with accessible semantics', () => {
    render(<MatchList matches={[baseMatch]} />);
    expect(screen.getByRole('list', { name: 'Match list' })).toBeTruthy();
    expect(screen.getAllByRole('listitem')).toHaveLength(1);
  });

  it('renders one card per match', () => {
    const matches = [baseMatch, { ...baseMatch, matchId: 2 }];
    render(<MatchList matches={matches} />);
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
  });

  it('list items are keyboard focusable', () => {
    render(<MatchList matches={[baseMatch]} />);
    const item = screen.getByRole('listitem');
    expect(item.getAttribute('tabindex')).toBe('0');
  });

  it('snapshot: populated list', () => {
    const { container } = render(<MatchList matches={[baseMatch]} />);
    expect(container).toMatchSnapshot();
  });
});
