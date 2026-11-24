import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MatchDetailPage } from './MatchDetailPage';

describe('MatchDetailPage', () => {
  it('renders match detail page with default values', () => {
    render(<MatchDetailPage matchId={42} />);
    expect(screen.getByRole('heading', { level: 2 }).textContent).toBe('Match #42');
    expect(screen.queryByText('Dispute Status')).toBeNull();
  });

  it('renders dispute status when open', () => {
    render(<MatchDetailPage matchId={42} disputeStatus="open" />);
    expect(screen.getByText('Dispute Status')).toBeInTheDocument();
    const disputeElement = screen.getByTestId('dispute-status');
    expect(disputeElement).toBeInTheDocument();
    expect(disputeElement.textContent).toBe('Open');
  });

  it('renders dispute status when resolved', () => {
    render(<MatchDetailPage matchId={42} disputeStatus="resolved" />);
    expect(screen.getByText('Dispute Status')).toBeInTheDocument();
    const disputeElement = screen.getByTestId('dispute-status');
    expect(disputeElement).toBeInTheDocument();
    expect(disputeElement.textContent).toBe('Resolved');
  });

  it('does not render dispute status when undefined', () => {
    render(<MatchDetailPage matchId={42} disputeStatus={undefined} />);
    expect(screen.queryByText('Dispute Status')).toBeNull();
    expect(screen.queryByTestId('dispute-status')).toBeNull();
  });
});
