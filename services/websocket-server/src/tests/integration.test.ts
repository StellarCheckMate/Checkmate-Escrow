/**
 * Integration tests: WebSocket server ↔ event-indexer
 *
 * These tests spin up:
 *   1. A lightweight HTTP mock of the event-indexer REST API
 *   2. The real EventPoller wired to that mock
 *   3. The real ConnectionManager (WebSocket server)
 *
 * They verify the full end-to-end flow: event-indexer emits events →
 * EventPoller picks them up → ConnectionManager broadcasts to clients →
 * clients receive the event over their WebSocket connection.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import WebSocket from 'ws';
import http from 'http';
import { ConnectionManager } from '../connectionManager.js';
import { EventPoller } from '../eventPoller.js';
import type { IndexedEvent, ServerConfig } from '../types.js';

// ─── Port registry — avoid conflicts between tests ────────────────────────

let nextPort = 9200;
function allocPort(): number { return nextPort++; }

// ─── Helpers ──────────────────────────────────────────────────────────────

function buildConfig(wsPort: number, indexerPort: number): ServerConfig {
  return {
    port: wsPort,
    host: '127.0.0.1',
    eventIndexerUrl: `http://127.0.0.1:${indexerPort}`,
    pollIntervalMs: 100,         // fast polling for tests
    heartbeatIntervalMs: 60_000,
    heartbeatTimeoutMs: 120_000,
    rateLimitMaxSubscribes: 100,
    rateLimitWindowMs: 60_000,
    maxSubscriptionsPerClient: 50,
    logLevel: 'warn',
  };
}

/**
 * Minimal HTTP server that returns the configured events on GET /events.
 * Supports dynamically updating the events list via `setEvents`.
 */
function createMockIndexer(port: number): {
  setEvents: (events: IndexedEvent[]) => void;
  stop: () => Promise<void>;
} {
  let currentEvents: IndexedEvent[] = [];
  const server = http.createServer((_req, res) => {
    if (currentEvents.length === 0) {
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ success: false, data: null, error: 'No events found' }));
    } else {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ success: true, data: currentEvents, error: null }));
    }
  });

  server.listen(port);

  return {
    setEvents: (events) => { currentEvents = events; },
    stop: () =>
      new Promise((resolve, reject) =>
        server.close((err) => (err ? reject(err) : resolve())),
      ),
  };
}

function connectClient(wsPort: number): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${wsPort}`);
    ws.once('open', () => resolve(ws));
    ws.once('error', reject);
  });
}

function waitForMessage(ws: WebSocket, type: string, timeoutMs = 5_000): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`Timed out waiting for message type '${type}'`)),
      timeoutMs,
    );
    ws.on('message', function handler(raw) {
      try {
        const msg = JSON.parse(raw.toString()) as { type: string };
        if (msg.type === type) {
          clearTimeout(timer);
          ws.off('message', handler);
          resolve(msg);
        }
      } catch {/* ignore */}
    });
  });
}

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: `evt-${Date.now()}`,
    ledger_sequence: 1000,
    match_id: 1,
    event_type: 'match/created',
    player1: 'GABC',
    player2: 'GDEF',
    timestamp: new Date().toISOString(),
    ...overrides,
  };
}

// ─── Test suite ───────────────────────────────────────────────────────────

describe('WebSocket server ↔ event-indexer integration', () => {
  let wsPort: number;
  let indexerPort: number;
  let indexer: ReturnType<typeof createMockIndexer>;
  let manager: ConnectionManager;
  let poller: EventPoller;

  beforeEach(async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });

    wsPort = allocPort();
    indexerPort = allocPort();

    indexer = createMockIndexer(indexerPort);
    const config = buildConfig(wsPort, indexerPort);

    manager = new ConnectionManager(config);
    manager.start();

    poller = new EventPoller(config, (event) => manager.broadcast(event));
  });

  afterEach(async () => {
    poller.stop();
    await manager.stop();
    await indexer.stop();
    vi.useRealTimers();
  });

  // ── Connection handshake ───────────────────────────────────────────────

  it('sends protocol version 1 welcome on connect', async () => {
    const ws = await connectClient(wsPort);
    const msg = (await waitForMessage(ws, 'welcome')) as { protocol_version: number };
    expect(msg.protocol_version).toBe(1);
    ws.close();
  });

  it('acknowledges subscription with subscribed message', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');

    ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [42] } }));
    const sub = (await waitForMessage(ws, 'subscribed')) as { match_ids: number[] };
    expect(sub.match_ids).toContain(42);
    ws.close();
  });

  // ── Event delivery ─────────────────────────────────────────────────────

  it('delivers a polled event to a subscribed client', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');
    ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [7] } }));
    await waitForMessage(ws, 'subscribed');

    // Prime the mock indexer with an event
    indexer.setEvents([makeEvent({ match_id: 7, ledger_sequence: 2000 })]);

    // Start polling — event should arrive
    poller.start();

    const eventMsg = (await waitForMessage(ws, 'event', 3_000)) as { event: IndexedEvent };
    expect(eventMsg.event.match_id).toBe(7);
    expect(eventMsg.event.event_type).toBe('match/created');
    ws.close();
  });

  it('does NOT deliver event to client subscribed to different match', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');
    ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [99] } }));
    await waitForMessage(ws, 'subscribed');

    indexer.setEvents([makeEvent({ match_id: 7, ledger_sequence: 2001 })]);
    poller.start();

    // Wait 500 ms — should not receive an event
    const received = await new Promise<boolean>((resolve) => {
      const timer = setTimeout(() => resolve(false), 500);
      ws.on('message', (raw) => {
        const msg = JSON.parse(raw.toString()) as { type: string };
        if (msg.type === 'event') {
          clearTimeout(timer);
          resolve(true);
        }
      });
    });

    expect(received).toBe(false);
    ws.close();
  });

  it('delivers event to client subscribed by player address', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');
    ws.send(JSON.stringify({
      type: 'subscribe',
      payload: { player_addresses: ['GABC'] },
    }));
    await waitForMessage(ws, 'subscribed');

    indexer.setEvents([makeEvent({ match_id: 5, player1: 'GABC', ledger_sequence: 3000 })]);
    poller.start();

    const eventMsg = (await waitForMessage(ws, 'event', 3_000)) as { event: IndexedEvent };
    expect(eventMsg.event.player1).toBe('GABC');
    ws.close();
  });

  it('does not deliver the same event twice', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');
    ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [7] } }));
    await waitForMessage(ws, 'subscribed');

    const event = makeEvent({ match_id: 7, ledger_sequence: 4000 });
    indexer.setEvents([event]);
    poller.start();

    let eventCount = 0;
    ws.on('message', (raw) => {
      const msg = JSON.parse(raw.toString()) as { type: string };
      if (msg.type === 'event') eventCount += 1;
    });

    // First poll delivers the event
    await waitForMessage(ws, 'event', 3_000);

    // Wait for a second poll cycle — event should not re-deliver
    await new Promise((r) => setTimeout(r, 300));

    expect(eventCount).toBe(1);
    ws.close();
  });

  // ── Error handling ─────────────────────────────────────────────────────

  it('returns error message for invalid JSON', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');

    ws.send('this is not json');
    const err = (await waitForMessage(ws, 'error')) as { code: string };
    expect(err.code).toBe('PARSE_ERROR');
    ws.close();
  });

  it('returns error message for unknown message type', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');

    ws.send(JSON.stringify({ type: 'bogus' }));
    const err = (await waitForMessage(ws, 'error')) as { code: string };
    expect(err.code).toBe('UNKNOWN_MESSAGE_TYPE');
    ws.close();
  });

  it('responds to ping with pong', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');

    ws.send(JSON.stringify({ type: 'ping' }));
    const pong = (await waitForMessage(ws, 'pong')) as { server_time: string };
    expect(typeof pong.server_time).toBe('string');
    ws.close();
  });

  // ── Unsubscribe ────────────────────────────────────────────────────────

  it('stops delivering events after unsubscribe', async () => {
    const ws = await connectClient(wsPort);
    await waitForMessage(ws, 'welcome');
    ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [7] } }));
    await waitForMessage(ws, 'subscribed');

    // Unsubscribe before any events arrive
    ws.send(JSON.stringify({ type: 'unsubscribe', payload: { match_ids: [7] } }));
    await waitForMessage(ws, 'unsubscribed');

    indexer.setEvents([makeEvent({ match_id: 7, ledger_sequence: 5000 })]);
    poller.start();

    const received = await new Promise<boolean>((resolve) => {
      const timer = setTimeout(() => resolve(false), 500);
      ws.on('message', (raw) => {
        const msg = JSON.parse(raw.toString()) as { type: string };
        if (msg.type === 'event') { clearTimeout(timer); resolve(true); }
      });
    });

    expect(received).toBe(false);
    ws.close();
  });

  // ── Rate limiting ──────────────────────────────────────────────────────

  it('rate-limits clients that exceed subscribe command threshold', async () => {
    // Build a very tight rate-limit config
    const tightWsPort = allocPort();
    const tightCfg = buildConfig(tightWsPort, indexerPort);
    tightCfg.rateLimitMaxSubscribes = 2;
    tightCfg.rateLimitWindowMs = 60_000;

    const tightManager = new ConnectionManager(tightCfg);
    tightManager.start();

    try {
      const ws = await connectClient(tightWsPort);
      await waitForMessage(ws, 'welcome');

      // 2 subscribes should work
      ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [1] } }));
      await waitForMessage(ws, 'subscribed');
      ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [2] } }));
      await waitForMessage(ws, 'subscribed');

      // 3rd should be rate-limited
      ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [3] } }));
      const rateLimited = (await waitForMessage(ws, 'rate_limited')) as { retry_after_ms: number };
      expect(rateLimited.retry_after_ms).toBeGreaterThan(0);
      ws.close();
    } finally {
      await tightManager.stop();
    }
  });
});
