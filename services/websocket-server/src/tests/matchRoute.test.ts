/**
 * Tests for #966 — /ws/match/:match_id auto-subscription URL route.
 *
 * Verifies that:
 * - Connecting to /ws/match/42 automatically subscribes the client to match 42.
 * - The server sends a `subscribed` acknowledgement with match_id=42.
 * - Events for match 42 are delivered to the auto-subscribed client.
 * - Connecting to / (root) does NOT auto-subscribe.
 * - Invalid paths (non-numeric, negative) are ignored gracefully.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import WebSocket from 'ws';
import { ConnectionManager } from '../connectionManager.js';
import { parseMatchIdFromUrl } from '../connectionManager.js';
import type { IndexedEvent, ServerConfig } from '../types.js';

// ─── Port registry ───────────────────────────────────────────────────────────

let nextPort = 9400;
function allocPort(): number { return nextPort++; }

// ─── Config helper ───────────────────────────────────────────────────────────

function buildConfig(port: number): ServerConfig {
  return {
    port,
    host: '127.0.0.1',
    eventIndexerUrl: 'http://127.0.0.1:19999', // unused in these tests
    pollIntervalMs: 60_000,
    heartbeatIntervalMs: 60_000,
    heartbeatTimeoutMs: 120_000,
    rateLimitMaxSubscribes: 100,
    rateLimitWindowMs: 60_000,
    maxSubscriptionsPerClient: 50,
    logLevel: 'warn',
  };
}

// ─── Client helper ───────────────────────────────────────────────────────────

function connectClient(port: number, path: string = '/'): WebSocket {
  return new WebSocket(`ws://127.0.0.1:${port}${path}`);
}

function waitForMessage(ws: WebSocket, type: string, timeoutMs = 2000): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`Timeout waiting for message type=${type}`)), timeoutMs);
    ws.on('message', (data) => {
      const msg = JSON.parse(data.toString()) as Record<string, unknown>;
      if (msg.type === type) {
        clearTimeout(timer);
        resolve(msg);
      }
    });
  });
}

// ─── parseMatchIdFromUrl unit tests ──────────────────────────────────────────

describe('parseMatchIdFromUrl', () => {
  it('returns the match_id for /ws/match/42', () => {
    expect(parseMatchIdFromUrl('/ws/match/42')).toBe(42);
  });

  it('returns the match_id for /ws/match/0', () => {
    expect(parseMatchIdFromUrl('/ws/match/0')).toBe(0);
  });

  it('accepts trailing slash /ws/match/42/', () => {
    expect(parseMatchIdFromUrl('/ws/match/42/')).toBe(42);
  });

  it('returns null for root path /', () => {
    expect(parseMatchIdFromUrl('/')).toBeNull();
  });

  it('returns null for /ws/match/ (no id)', () => {
    expect(parseMatchIdFromUrl('/ws/match/')).toBeNull();
  });

  it('returns null for non-numeric id /ws/match/abc', () => {
    expect(parseMatchIdFromUrl('/ws/match/abc')).toBeNull();
  });

  it('returns null for /events path', () => {
    expect(parseMatchIdFromUrl('/events')).toBeNull();
  });

  it('handles large match IDs', () => {
    expect(parseMatchIdFromUrl('/ws/match/9999999')).toBe(9999999);
  });
});

// ─── Integration: auto-subscription via URL ──────────────────────────────────

describe('WebSocket /ws/match/:match_id auto-subscription', () => {
  let manager: ConnectionManager;
  let port: number;

  beforeEach(() => {
    port = allocPort();
    manager = new ConnectionManager(buildConfig(port));
    manager.start();
  });

  afterEach(async () => {
    await manager.stop();
  });

  it('connecting to /ws/match/42 receives subscribed message for match 42', async () => {
    const ws = connectClient(port, '/ws/match/42');

    try {
      // First message is welcome
      await waitForMessage(ws, 'welcome');

      // Second message should be the auto-subscription acknowledgement
      const subscribed = await waitForMessage(ws, 'subscribed');
      expect(subscribed.match_ids).toEqual([42]);
      expect(subscribed.player_addresses).toEqual([]);
    } finally {
      ws.close();
    }
  });

  it('connecting to / (root) does not send a subscribed message automatically', async () => {
    const ws = connectClient(port, '/');

    try {
      await waitForMessage(ws, 'welcome');

      // No subscribed message should arrive — wait briefly then confirm none came
      let receivedSubscribed = false;
      ws.on('message', (data) => {
        const msg = JSON.parse(data.toString()) as Record<string, unknown>;
        if (msg.type === 'subscribed') receivedSubscribed = true;
      });

      await new Promise((r) => setTimeout(r, 300));
      expect(receivedSubscribed).toBe(false);
    } finally {
      ws.close();
    }
  });

  it('broadcasts events to auto-subscribed client', async () => {
    const ws = connectClient(port, '/ws/match/99');

    try {
      await waitForMessage(ws, 'welcome');
      await waitForMessage(ws, 'subscribed');

      // Manually broadcast an event for match 99
      const event: IndexedEvent = {
        id: 'evt-001',
        ledger_sequence: 1000,
        match_id: 99,
        event_type: 'match/completed',
        player1: 'GAAA',
        player2: 'GBBB',
        status: 'completed',
        winner: 'player1',
        stake_amount: '500',
        token: 'XLM',
        game_id: 'abcd1234',
        platform: 'Lichess',
        timestamp: new Date().toISOString(),
        txn_hash: 'txhash123',
        event_index_in_txn: 0,
      };

      manager.broadcast(event);

      const received = await waitForMessage(ws, 'event');
      expect((received.event as IndexedEvent).match_id).toBe(99);
      expect((received.event as IndexedEvent).event_type).toBe('match/completed');
    } finally {
      ws.close();
    }
  });

  it('auto-subscribed client can still send additional subscribe messages', async () => {
    const ws = connectClient(port, '/ws/match/10');

    try {
      await waitForMessage(ws, 'welcome');
      await waitForMessage(ws, 'subscribed'); // auto-sub for match 10

      // Manually subscribe to match 20 as well
      ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [20] } }));
      const subscribed2 = await waitForMessage(ws, 'subscribed');

      // Should now be subscribed to both 10 and 20
      expect((subscribed2.match_ids as number[]).sort()).toEqual([10, 20]);
    } finally {
      ws.close();
    }
  });
});
