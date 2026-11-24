import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MatchDetail, buildMatchLink } from './MatchDetail';

const baseProps = {
  matchId: 1234,
  player1: 'GAAA',
  player2: 'GBBB',
  stakeAmount: '50',
  token: 'USDC',
  status: 'active' as const,
  platform: 'lichess' as const,
};

describe('MatchDetail - Copy Link', () => {
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
  });

  it('builds the correct deep-link URL', () => {
    expect(buildMatchLink(1234, 'https://app.checkmate-escrow.xyz')).toBe(
      'https://app.checkmate-escrow.xyz/match/1234',
    );
  });

  it('copies the correct match URL to the clipboard when clicked', async () => {
    render(<MatchDetail {...baseProps} />);

    fireEvent.click(screen.getByRole('button', { name: 'Copy Link' }));

    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText).toHaveBeenCalledWith(`${window.location.origin}/match/1234`);
  });

  it('shows confirmation text after copying', async () => {
    render(<MatchDetail {...baseProps} />);

    fireEvent.click(screen.getByRole('button', { name: 'Copy Link' }));

    await waitFor(() => expect(screen.getByRole('button').textContent).toBe('Link copied!'));
  });
});

describe('MatchDetail - Dispute Status', () => {
  it('does not display dispute status when not provided', () => {
    render(<MatchDetail {...baseProps} />);
    expect(screen.queryByText('Dispute Status')).toBeNull();
    expect(screen.queryByTestId('dispute-status')).toBeNull();
  });

  it('does not display dispute status when null', () => {
    render(<MatchDetail {...baseProps} disputeStatus={null} />);
    expect(screen.queryByText('Dispute Status')).toBeNull();
    expect(screen.queryByTestId('dispute-status')).toBeNull();
  });

  it('renders open dispute status correctly', () => {
    render(<MatchDetail {...baseProps} disputeStatus="open" />);
    expect(screen.getByText('Dispute Status')).toBeInTheDocument();
    const statusElem = screen.getByTestId('dispute-status');
    expect(statusElem).toBeInTheDocument();
    expect(statusElem.textContent).toBe('Open');
  });

  it('renders resolved dispute status correctly', () => {
    render(<MatchDetail {...baseProps} disputeStatus="resolved" />);
    expect(screen.getByText('Dispute Status')).toBeInTheDocument();
    const statusElem = screen.getByTestId('dispute-status');
    expect(statusElem).toBeInTheDocument();
    expect(statusElem.textContent).toBe('Resolved');
  });
});
