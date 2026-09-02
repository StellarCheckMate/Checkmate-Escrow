import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import { MatchList } from '../components/MatchList';

// ─── Mock WebSocket (mirrors useMatchWebSocket.test.ts) ───────────────────

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readonly url: string;
  readyState = MockWebSocket.CONNECTING;

  onopen: (() => void) | null = null;
  onmessage: ((evt: { data: string }) => void) | null = null;
  onclose: ((evt: { code: number; reason: string }) => void) | null = null;
  onerror: (() => void) | null = null;

  private static instances: MockWebSocket[] = [];

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send(): void {}
  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ code: 1000, reason: 'Normal closure' });
  }

  simulateOpen(): void {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }
  simulateMessage(msg: object): void {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }

  static lastInstance(): MockWebSocket {
    return MockWebSocket.instances[MockWebSocket.instances.length - 1];
  }
  static reset(): void {
    MockWebSocket.instances = [];
  }
}

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

  describe('live refresh via WebSocket', () => {
    beforeEach(() => {
      MockWebSocket.reset();
      (globalThis as unknown as Record<string, unknown>).WebSocket = MockWebSocket;
    });

    afterEach(() => {
      delete (globalThis as unknown as Record<string, unknown>).WebSocket;
    });

    it('calls onRefresh when a deposit event arrives', async () => {
      const onRefresh = vi.fn();
      render(<MatchList matches={[baseMatch]} onRefresh={onRefresh} />);

      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() =>
        ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }),
      );
      act(() =>
        ws.simulateMessage({
          type: 'event',
          event: {
            id: 'evt-1',
            ledger_sequence: 1,
            match_id: baseMatch.matchId,
            event_type: 'deposit',
            timestamp: new Date().toISOString(),
          },
        }),
      );

      await waitFor(() => expect(onRefresh).toHaveBeenCalled());
    });

    it('calls onRefresh when a result_submitted event arrives', async () => {
      const onRefresh = vi.fn();
      render(<MatchList matches={[baseMatch]} onRefresh={onRefresh} />);

      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() =>
        ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }),
      );
      act(() =>
        ws.simulateMessage({
          type: 'event',
          event: {
            id: 'evt-2',
            ledger_sequence: 2,
            match_id: baseMatch.matchId,
            event_type: 'result_submitted',
            timestamp: new Date().toISOString(),
          },
        }),
      );

      await waitFor(() => expect(onRefresh).toHaveBeenCalled());
    });

    it('does not call onRefresh for unrelated event types', async () => {
      const onRefresh = vi.fn();
      render(<MatchList matches={[baseMatch]} onRefresh={onRefresh} />);

      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() =>
        ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }),
      );
      act(() =>
        ws.simulateMessage({
          type: 'event',
          event: {
            id: 'evt-3',
            ledger_sequence: 3,
            match_id: baseMatch.matchId,
            event_type: 'subscribed_noop',
            timestamp: new Date().toISOString(),
          },
        }),
      );

      expect(onRefresh).not.toHaveBeenCalled();
    });
  });
});
