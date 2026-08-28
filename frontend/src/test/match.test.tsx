import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MatchStatusBadge } from '../components/match/MatchStatusBadge';
import { DepositButton } from '../components/match/DepositButton';
import { CreateMatchForm } from '../components/match/CreateMatchForm';
import { MatchCard } from '../components/match/MatchCard';

// ── MatchStatusBadge ────────────────────────────────────────────────────────

describe('MatchStatusBadge', () => {
  it.each(['pending', 'active', 'completed', 'cancelled'] as const)('snapshot: %s', status => {
    const { container } = render(<MatchStatusBadge status={status} />);
    expect(container).toMatchSnapshot();
  });

  it('has aria-label with status text', () => {
    render(<MatchStatusBadge status="active" />);
    expect(screen.getByLabelText('active')).toBeTruthy();
  });
});

// ── DepositButton ────────────────────────────────────────────────────────────

describe('DepositButton', () => {
  it('snapshot: idle', () => {
    const { container } = render(<DepositButton matchId={1} onDeposit={vi.fn()} />);
    expect(container).toMatchSnapshot();
  });

  it('snapshot: disabled', () => {
    const { container } = render(<DepositButton matchId={1} disabled onDeposit={vi.fn()} />);
    expect(container).toMatchSnapshot();
  });

  it('shows success feedback after deposit', async () => {
    const onDeposit = vi.fn().mockResolvedValue(undefined);
    render(<DepositButton matchId={2} onDeposit={onDeposit} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => screen.getByRole('status'));
    expect(screen.getByRole('status').textContent).toBe('Deposit successful!');
  });

  it('shows error feedback on failure', async () => {
    const onDeposit = vi.fn().mockRejectedValue(new Error('tx failed'));
    render(<DepositButton matchId={3} onDeposit={onDeposit} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => screen.getByRole('status'));
    expect(screen.getByRole('status').textContent).toBe('tx failed');
  });
});

// ── CreateMatchForm ──────────────────────────────────────────────────────────

describe('CreateMatchForm', () => {
  it('snapshot: empty form', () => {
    const { container } = render(<CreateMatchForm onSubmit={vi.fn()} />);
    expect(container).toMatchSnapshot();
  });

  it('shows validation errors on empty submit', () => {
    render(<CreateMatchForm onSubmit={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /create match/i }));
    expect(screen.getAllByRole('alert').length).toBeGreaterThan(0);
  });

  it('shows error for invalid lichess game ID and blocks submit', () => {
    const onSubmit = vi.fn();
    render(<CreateMatchForm onSubmit={onSubmit} />);
    fireEvent.change(screen.getByLabelText(/player 2/i), { target: { value: 'GBOB' } });
    fireEvent.change(screen.getByLabelText(/stake amount/i), { target: { value: '10' } });
    fireEvent.change(screen.getByLabelText(/token address/i), { target: { value: 'GTOK' } });
    fireEvent.change(screen.getByLabelText(/game id/i), { target: { value: 'abc12' } }); // only 5 chars
    fireEvent.click(screen.getByRole('button', { name: /create match/i }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(/Lichess game ID must be exactly 8 alphanumeric characters/i)).toBeInTheDocument();
  });

  it('shows error for invalid chess.com game ID and blocks submit', () => {
    const onSubmit = vi.fn();
    render(<CreateMatchForm onSubmit={onSubmit} />);
    fireEvent.change(screen.getByLabelText(/player 2/i), { target: { value: 'GBOB' } });
    fireEvent.change(screen.getByLabelText(/stake amount/i), { target: { value: '10' } });
    fireEvent.change(screen.getByLabelText(/token address/i), { target: { value: 'GTOK' } });
    fireEvent.change(screen.getByLabelText(/platform/i), { target: { value: 'chessdotcom' } });
    fireEvent.change(screen.getByLabelText(/game id/i), { target: { value: 'short' } });
    fireEvent.click(screen.getByRole('button', { name: /create match/i }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(/Chess.com game ID must be 7-12 digits/i)).toBeInTheDocument();
  });

  it('calls onSubmit with form data when valid (Lichess)', () => {
    const onSubmit = vi.fn();
    render(<CreateMatchForm onSubmit={onSubmit} />);
    fireEvent.change(screen.getByLabelText(/player 2/i), { target: { value: 'GBOB' } });
    fireEvent.change(screen.getByLabelText(/stake amount/i), { target: { value: '10' } });
    fireEvent.change(screen.getByLabelText(/token address/i), { target: { value: 'GTOK' } });
    fireEvent.change(screen.getByLabelText(/game id/i), { target: { value: 'abc12345' } });
    fireEvent.click(screen.getByRole('button', { name: /create match/i }));
    expect(onSubmit).toHaveBeenCalledWith({
      player2: 'GBOB',
      stakeAmount: '10',
      token: 'GTOK',
      gameId: 'abc12345',
      platform: 'lichess',
    });
  });

  it('calls onSubmit with form data when valid (Chess.com)', () => {
    const onSubmit = vi.fn();
    render(<CreateMatchForm onSubmit={onSubmit} />);
    fireEvent.change(screen.getByLabelText(/player 2/i), { target: { value: 'GBOB' } });
    fireEvent.change(screen.getByLabelText(/stake amount/i), { target: { value: '10' } });
    fireEvent.change(screen.getByLabelText(/token address/i), { target: { value: 'GTOK' } });
    fireEvent.change(screen.getByLabelText(/platform/i), { target: { value: 'chessdotcom' } });
    fireEvent.change(screen.getByLabelText(/game id/i), { target: { value: '1234567890' } });
    fireEvent.click(screen.getByRole('button', { name: /create match/i }));
    expect(onSubmit).toHaveBeenCalledWith({
      player2: 'GBOB',
      stakeAmount: '10',
      token: 'GTOK',
      gameId: '1234567890',
      platform: 'chessdotcom',
    });
  });
});

// ── MatchCard ────────────────────────────────────────────────────────────────

describe('MatchCard', () => {
  const baseProps = {
    matchId: 42,
    player1: 'GAAA',
    player2: 'GBBB',
    stakeAmount: '50',
    token: 'USDC',
    status: 'active' as const,
    platform: 'lichess' as const,
  };

  it('snapshot', () => {
    const { container } = render(<MatchCard {...baseProps} />);
    expect(container).toMatchSnapshot();
  });

  it('renders status badge', () => {
    render(<MatchCard {...baseProps} />);
    expect(screen.getByLabelText('active')).toBeTruthy();
  });
});
