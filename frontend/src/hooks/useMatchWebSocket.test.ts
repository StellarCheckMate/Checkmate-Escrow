/**
 * Unit tests for useMatchWebSocket
 *
 * Uses a mock WebSocket class (injected via global) so no real network is
 * needed.  Tests cover:
 *   - Welcome handshake + auto-subscribe
 *   - Status transitions
 *   - Incoming event delivery
 *   - Exponential back-off reconnect
 *   - enabled=false disables connection
 *   - reconnect() public API
 */

import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { useMatchWebSocket } from '../useMatchWebSocket';

// ─── Mock WebSocket ────────────────────────────────────────────────────────

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

  send(_data: string): void {}
  close(_code?: number, _reason?: string): void {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ code: 1000, reason: 'Normal closure' });
  }

  /** Test helpers */
  simulateOpen(): void {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }
  simulateMessage(msg: object): void {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
  simulateClose(code = 1006, reason = 'connection lost'): void {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ code, reason });
  }
  simulateError(): void {
    this.onerror?.();
  }

  static lastInstance(): MockWebSocket {
    return MockWebSocket.instances[MockWebSocket.instances.length - 1];
  }
  static reset(): void {
    MockWebSocket.instances = [];
  }
}

// ─── Setup / teardown ──────────────────────────────────────────────────────

beforeEach(() => {
  MockWebSocket.reset();
  vi.useFakeTimers();
  // Inject mock into global
  (global as unknown as Record<string, unknown>).WebSocket = MockWebSocket;
});

afterEach(() => {
  vi.useRealTimers();
  delete (global as unknown as Record<string, unknown>).WebSocket;
});

// ─── Tests ─────────────────────────────────────────────────────────────────

describe('useMatchWebSocket', () => {

  describe('initial connection', () => {
    it('starts with status "connecting"', () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', enabled: true }),
      );
      expect(result.current.status).toBe('connecting');
    });

    it('transitions to "subscribing" after welcome (with matchIds)', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', matchIds: [1] }),
      );

      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: new Date().toISOString() }));

      await waitFor(() => expect(result.current.status).toBe('subscribing'));
    });

    it('transitions to "ready" after welcome (no subscriptions)', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090' }),
      );

      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: new Date().toISOString() }));

      await waitFor(() => expect(result.current.status).toBe('ready'));
    });

    it('transitions to "ready" after subscribed message', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', matchIds: [42] }),
      );

      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));
      act(() => ws.simulateMessage({ type: 'subscribed', match_ids: [42], player_addresses: [] }));

      await waitFor(() => expect(result.current.status).toBe('ready'));
    });
  });

  describe('event delivery', () => {
    const mockEvent = {
      id: 'evt-1',
      ledger_sequence: 100,
      match_id: 42,
      event_type: 'match/created',
      player1: 'GABC',
      player2: 'GDEF',
      timestamp: '2024-01-01T00:00:00Z',
    };

    async function connectAndReady() {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', matchIds: [42] }),
      );
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));
      act(() => ws.simulateMessage({ type: 'subscribed', match_ids: [42], player_addresses: [] }));
      await waitFor(() => expect(result.current.status).toBe('ready'));
      return { result, ws };
    }

    it('populates latestEvent when event arrives', async () => {
      const { result, ws } = await connectAndReady();

      act(() => ws.simulateMessage({ type: 'event', event: mockEvent }));

      await waitFor(() => expect(result.current.latestEvent).not.toBeNull());
      expect(result.current.latestEvent?.match_id).toBe(42);
    });

    it('accumulates events in history array', async () => {
      const { result, ws } = await connectAndReady();

      act(() => ws.simulateMessage({ type: 'event', event: { ...mockEvent, id: 'evt-1' } }));
      act(() => ws.simulateMessage({ type: 'event', event: { ...mockEvent, id: 'evt-2' } }));

      await waitFor(() => expect(result.current.events).toHaveLength(2));
    });

    it('caps history at maxHistory', async () => {
      const maxHistory = 3;
      const { result, ws } = await connectAndReady();
      // Override maxHistory by rendering fresh
      const { result: r2 } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', matchIds: [42], maxHistory }),
      );
      const ws2 = MockWebSocket.lastInstance();
      act(() => ws2.simulateOpen());
      act(() => ws2.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));
      act(() => ws2.simulateMessage({ type: 'subscribed', match_ids: [42], player_addresses: [] }));
      await waitFor(() => expect(r2.current.status).toBe('ready'));

      for (let i = 0; i < 5; i++) {
        act(() => ws2.simulateMessage({ type: 'event', event: { ...mockEvent, id: `evt-${i}` } }));
      }

      await waitFor(() => expect(r2.current.events.length).toBeLessThanOrEqual(maxHistory));
      // Suppress unused variable warning for ws
      void ws;
      void result;
    });

    it('calls onEvent callback for each event', async () => {
      const onEvent = vi.fn();
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', matchIds: [42], onEvent }),
      );
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));
      act(() => ws.simulateMessage({ type: 'subscribed', match_ids: [42], player_addresses: [] }));
      await waitFor(() => expect(result.current.status).toBe('ready'));

      act(() => ws.simulateMessage({ type: 'event', event: mockEvent }));

      await waitFor(() => expect(onEvent).toHaveBeenCalledTimes(1));
      expect(onEvent).toHaveBeenCalledWith(expect.objectContaining({ match_id: 42 }));
    });
  });

  describe('reconnect behaviour', () => {
    it('transitions to "reconnecting" on close', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090' }),
      );
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));

      act(() => ws.simulateClose());
      await waitFor(() => expect(result.current.status).toBe('reconnecting'));
    });

    it('creates a new WebSocket after back-off delay', async () => {
      const initialCount = 1; // first socket created in renderHook
      renderHook(() => useMatchWebSocket({ url: 'ws://localhost:8090' }));
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));
      act(() => ws.simulateClose());

      // Advance timers to trigger reconnect (base 1s)
      act(() => vi.advanceTimersByTime(2_000));

      // A second socket should have been created
      await waitFor(() => {
        const allInstances = (MockWebSocket as unknown as { instances: MockWebSocket[] }).instances;
        expect(allInstances.length).toBeGreaterThan(initialCount);
      });
    });
  });

  describe('enabled flag', () => {
    it('does not create WebSocket when enabled=false', () => {
      renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', enabled: false }),
      );
      const allInstances = (MockWebSocket as unknown as { instances: MockWebSocket[] }).instances;
      expect(allInstances.length).toBe(0);
    });

    it('sets status to "closed" when enabled=false', () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', enabled: false }),
      );
      expect(result.current.status).toBe('closed');
    });
  });

  describe('reconnect() API', () => {
    it('resets retry count and reconnects immediately', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090' }),
      );
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));
      act(() => ws.simulateClose());

      await waitFor(() => expect(result.current.status).toBe('reconnecting'));

      act(() => result.current.reconnect());

      const allInstances = (MockWebSocket as unknown as { instances: MockWebSocket[] }).instances;
      await waitFor(() => expect(allInstances.length).toBeGreaterThan(1));
      expect(result.current.status).toBe('connecting');
    });
  });

  describe('error handling', () => {
    it('sets error on rate_limited message', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090', matchIds: [1] }),
      );
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));

      act(() =>
        ws.simulateMessage({
          type: 'rate_limited',
          message: 'Too many subscribe commands',
          retry_after_ms: 60000,
        }),
      );

      await waitFor(() => expect(result.current.error).not.toBeNull());
      expect(result.current.error).toMatch(/Too many/);
    });

    it('sets error on server error message', async () => {
      const { result } = renderHook(() =>
        useMatchWebSocket({ url: 'ws://localhost:8090' }),
      );
      const ws = MockWebSocket.lastInstance();
      act(() => ws.simulateOpen());
      act(() => ws.simulateMessage({ type: 'welcome', protocol_version: 1, server_time: '' }));

      act(() =>
        ws.simulateMessage({
          type: 'error',
          code: 'SUBSCRIBE_ERROR',
          message: 'Subscription cap exceeded',
        }),
      );

      await waitFor(() => expect(result.current.error).not.toBeNull());
      expect(result.current.error).toMatch(/SUBSCRIBE_ERROR/);
    });
  });
});
