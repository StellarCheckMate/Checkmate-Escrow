/**
 * Tests for the MatchReceiptPDF component and the MatchDetail integration
 * (issue #1432 — PDF export for match payout receipts).
 *
 * @react-pdf/renderer renders to a non-DOM tree, so we test:
 *  1. MatchReceiptDocument exposes the correct props for content assertions.
 *  2. MatchDetail shows a "Download Receipt" button only for completed matches.
 *  3. The download helper is triggered when the button is clicked.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MatchDetail } from './MatchDetail';
import type { MatchReceiptProps } from './MatchReceiptPDF';

// ── Mock @react-pdf/renderer ──────────────────────────────────────────────────
// The renderer uses Canvas / Worker APIs unavailable in jsdom; mock it so the
// structural tests run without needing a headless browser environment.
vi.mock('@react-pdf/renderer', () => ({
  Document: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  Page: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  Text: ({ children }: { children: React.ReactNode }) => <span>{children}</span>,
  View: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  StyleSheet: { create: (s: unknown) => s },
  pdf: vi.fn().mockReturnValue({ toBlob: vi.fn().mockResolvedValue(new Blob()) }),
}));

// ── Mock the lazy import inside MatchDetail ───────────────────────────────────
// vi.mock hoists to the top of the module, so it intercepts the dynamic
// import('./MatchReceiptPDF') inside the click handler too.
const mockDownload = vi.fn().mockResolvedValue(undefined);
vi.mock('./MatchReceiptPDF', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./MatchReceiptPDF')>();
  return {
    ...actual,
    downloadMatchReceipt: mockDownload,
  };
});

// ── Shared fixtures ───────────────────────────────────────────────────────────

const receipt: Omit<MatchReceiptProps, 'matchId'> = {
  completedAt: '2026-09-04T07:00:00.000Z',
  player1: 'GAAA',
  player2: 'GBBB',
  stakeAmount: '50',
  token: 'USDC',
  payoutAmount: '100',
  winner: 'GAAA',
  txHash: 'abc123def456',
};

const completedProps = {
  matchId: 7777,
  player1: 'GAAA',
  player2: 'GBBB',
  stakeAmount: '50',
  token: 'USDC',
  status: 'completed' as const,
  platform: 'lichess' as const,
  receipt,
};

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('MatchDetail — Download Receipt button', () => {
  beforeEach(() => {
    mockDownload.mockClear();
  });

  it('shows "Download Receipt" button for a completed match with receipt data', () => {
    render(<MatchDetail {...completedProps} />);
    // The button text is "Download Receipt"; aria-label is more descriptive.
    // Query by the aria-label which is the accessible name.
    expect(
      screen.getByRole('button', { name: 'Download payout receipt as PDF' }),
    ).toBeInTheDocument();
  });

  it('does NOT show "Download Receipt" button for a non-completed match', () => {
    render(<MatchDetail {...completedProps} status="active" />);
    expect(
      screen.queryByRole('button', { name: 'Download payout receipt as PDF' }),
    ).not.toBeInTheDocument();
  });

  it('does NOT show "Download Receipt" button when receipt prop is absent', () => {
    const { receipt: _omit, ...propsNoReceipt } = completedProps;
    render(<MatchDetail {...propsNoReceipt} />);
    expect(
      screen.queryByRole('button', { name: 'Download payout receipt as PDF' }),
    ).not.toBeInTheDocument();
  });

  it('calls downloadMatchReceipt with matchId and receipt data when clicked', async () => {
    render(<MatchDetail {...completedProps} />);

    fireEvent.click(screen.getByRole('button', { name: 'Download payout receipt as PDF' }));

    await waitFor(() => expect(mockDownload).toHaveBeenCalledTimes(1));
    expect(mockDownload).toHaveBeenCalledWith({
      matchId: 7777,
      ...receipt,
    });
  });

  it('shows "Generating…" while the download is in progress', async () => {
    // Make the mock stall so we can observe the interim label.
    let resolve!: () => void;
    mockDownload.mockReturnValueOnce(
      new Promise<void>((res) => { resolve = res; }),
    );

    render(<MatchDetail {...completedProps} />);
    fireEvent.click(screen.getByRole('button', { name: 'Download payout receipt as PDF' }));

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Download payout receipt as PDF' }).textContent).toBe('Generating…'),
    );

    // Unblock the mock and confirm the button label reverts.
    resolve();
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Download payout receipt as PDF' }).textContent).toBe('Download Receipt'),
    );
  });
});

describe('MatchReceiptDocument — content assertions', () => {
  it('renders match_id and payout amount in the document', async () => {
    // Import the un-mocked document component via the mocked module.
    const { MatchReceiptDocument } = await import('./MatchReceiptPDF');

    render(
      <MatchReceiptDocument
        matchId={7777}
        completedAt="2026-09-04T07:00:00.000Z"
        player1="GAAA"
        player2="GBBB"
        stakeAmount="50"
        token="USDC"
        payoutAmount="100"
        winner="GAAA"
        txHash="abc123def456"
      />,
    );

    // Match ID appears in the document (multiple occurrences are fine).
    const matchIdElements = screen.getAllByText(/7777/);
    expect(matchIdElements.length).toBeGreaterThan(0);

    // Payout amount appears in the document.
    const payoutElements = screen.getAllByText(/100/);
    expect(payoutElements.length).toBeGreaterThan(0);
  });
});
