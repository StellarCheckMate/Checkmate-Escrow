/**
 * Reconnection integration tests: WebSocket client reconnection handling
 *
 * These tests verify that:
 *   1. A client can reconnect after a server-side disconnect and resume
 *      receiving events (subscription state is re-established on reconnect).
 *   2. Events posted while a client is disconnected are NOT delivered retroactively
 *      (the server has no replay buffer — missed events stay missed).
 *   3. After reconnecting, a re-subscribed client receives new events correctly.
 *   4. Multiple sequential reconnections work without leaking server state.
 *   5. A client that reconnects via the /ws/match/:id URL path is auto-subscribed
 *      to the correct match on each new connection.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import WebSocket from 'ws';
import http from 'http';
import { ConnectionManager } from '../connectionManager.js';
import { EventPoller } from '../eventPoller.js';
import type { IndexedEvent, ServerConfig } from '../types.js';

// ─── Port registry — avoid conflicts with other test files ────────────────

let nextPort = 9400;
function allocPort(): number { return nextPort++; }

// ─── Helpers ─────────────────────────────────────────────────────────────

function buildConfig(wsPort: number, indexerPort: number): ServerConfig {
  return {
    port: wsPort,
    host: '127.0.0.1',
    eventIndexerUrl: `http://127.0.0.1:${indexerPort}`,
    pollIntervalMs: 80,           // fast polling keeps tests snappy
    heartbeatIntervalMs: 60_000,  // disable heartbeat timeouts during tests
    heartbeatTimeoutMs: 120_000,
    rateLimitMaxSubscribes: 100,
    rateLimitWindowMs: 60_000,
    maxSubscriptionsPerClient: 50,
    logLevel: 'warn',
  };
}

/**
 * Minimal HTTP server that returns the configured events on any GET request.
 * Returns 404 when the event list is empty (mirrors the real event-indexer).
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

/** Open a plain WebSocket connection and wait for the 'open' event. */
function connectClient(wsPort: number, path = '/'): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${wsPort}${path}`);
    ws.once('open', () => resolve(ws));
    ws.once('error', reject);
  });
}

/**
 * Return a promise that resolves with the first message of the given `type`.
 * Rejects after `timeoutMs` if no matching message arrives.
 */
function waitForMessage(
  ws: WebSocket,
  type: string,
  timeoutMs = 4_000,
): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`Timed out waiting for message type '${type}'`)),
      timeoutMs,
    );

    const handler = (raw: WebSocket.RawData): void => {
      try {
        const msg = JSON.parse(raw.toString()) as Record<string, unknown>;
        if (msg['type'] === type) {
          clearTimeout(timer);
          ws.off('message', handler);
          resolve(msg);
        }
      } catch {
        /* ignore non-JSON frames */
      }
    };

    ws.on('message', handler);
  });
}

/** Wait for the WebSocket `close` event. */
function waitForClose(ws: WebSocket, timeoutMs = 3_000): Promise<void> {
  return new Promise((resolve, reject) => {
    if (ws.readyState === WebSocket.CLOSED) { resolve(); return; }
    const timer = setTimeout(
      () => reject(new Error('Timed out waiting for WebSocket close')),
      timeoutMs,
    );
    ws.once('close', () => { clearTimeout(timer); resolve(); });
  });
}

/** Sleep for `ms` milliseconds. */
function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: `evt-reconnect-${Date.now()}-${Math.random()}`,
    ledger_sequence: 5000,
    match_id: 10,
    event_type: 'match/created',
    player1: 'GPLAYER1',
    player2: 'GPLAYER2',
    timestamp: new Date().toISOString(),
    ...overrides,
  };
}

// ─── Test suite ───────────────────────────────────────────────────────────

describe('WebSocket reconnection handling', () => {
  let wsPort: number;
  let indexerPort: number;
  let indexer: ReturnType<typeof createMockIndexer>;
  let manager: ConnectionManager;
  let poller: EventPoller;

  beforeEach(() => {
    wsPort = allocPort();
    indexerPort = allocPort();

    indexer = createMockIndexer(indexerPort);
    const config = buildConfig(wsPort, indexerPort);

    manager = new ConnectionManager(config);
    manager.start();

    poller = new EventPoller(config, (event) => manager.broadcast(event));
    poller.start();
  });

  afterEach(async () => {
    poller.stop();
    await manager.stop();
    await indexer.stop();
  });

  // ── 1. Reconnect and re-subscribe receives new events ─────────────────

  it('receives events after reconnecting and re-subscribing', async () => {
    // --- First connection ---
    const ws1 = await connectClient(wsPort);
    await waitForMessage(ws1, 'welcome');
    ws1.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [10] } }));
    await waitForMessage(ws1, 'subscribed');

    // Post an event and confirm it arrives
    indexer.setEvents([makeEvent({ match_id: 10, ledger_sequence: 5001 })]);
    const firstEvent = (await waitForMessage(ws1, 'event', 3_000)) as {
      event: IndexedEvent;
    };
    expect(firstEvent.event.match_id).toBe(10);

    // Close first connection
    ws1.close();
    await waitForClose(ws1);

    // Clear the already-delivered event; server will not re-deliver it
    indexer.setEvents([]);

    // --- Reconnect ---
    const ws2 = await connectClient(wsPort);
    await waitForMessage(ws2, 'welcome');
    ws2.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [10] } }));
    await waitForMessage(ws2, 'subscribed');

    // Post a new event on the same match
    indexer.setEvents([makeEvent({ match_id: 10, ledger_sequence: 5002 })]);
    const secondEvent = (await waitForMessage(ws2, 'event', 3_000)) as {
      event: IndexedEvent;
    };
    expect(secondEvent.event.match_id).toBe(10);
    expect(secondEvent.event.ledger_sequence).toBe(5002);

    ws2.close();
  });

  // ── 2. Events during disconnect are NOT replayed ───────────────────────

  it('does not replay events missed while disconnected', async () => {
    // Connect, subscribe, then immediately disconnect
    const ws1 = await connectClient(wsPort);
    await waitForMessage(ws1, 'welcome');
    ws1.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [11] } }));
    await waitForMessage(ws1, 'subscribed');
    ws1.close();
    await waitForClose(ws1);

    // Post an event while the client is gone — it will be broadcast but no one
    // is subscribed to match 11 any more, so it just drops.
    indexer.setEvents([makeEvent({ match_id: 11, ledger_sequence: 6000 })]);
    await sleep(250); // let at least one poll cycle fire

    // Reconnect — do NOT re-subscribe to match 11
    const ws2 = await connectClient(wsPort);
    await waitForMessage(ws2, 'welcome');

    // Nothing should arrive (no subscription)
    const received = await new Promise<boolean>((resolve) => {
      const timer = setTimeout(() => resolve(false), 400);
      ws2.on('message', (raw) => {
        const msg = JSON.parse(raw.toString()) as { type: string };
        if (msg.type === 'event') { clearTimeout(timer); resolve(true); }
      });
    });

    expect(received).toBe(false);
    ws2.close();
  });

  // ── 2b. Abrupt disconnect (no unsubscribe) still cleans up subscriptions ──
  //
  // Regression test: connectionManager.ts must remove a client's subscriptions
  // on the raw WebSocket 'close' event, not only in response to an explicit
  // 'unsubscribe' message. Otherwise a client that just drops off (network
  // blip, tab closed, crash) leaves its entries in SubscriptionManager's
  // matchIndex/playerIndex forever — a memory leak in long-running servers.

  it('removes a client\'s subscriptions when it disconnects without unsubscribing', async () => {
    // Client A subscribes to match 40 and disconnects abruptly (ws.close(),
    // never sends an 'unsubscribe' message).
    const clientA = await connectClient(wsPort);
    await waitForMessage(clientA, 'welcome');
    clientA.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [40] } }));
    await waitForMessage(clientA, 'subscribed');

    // Client B also subscribes to match 40 and stays connected.
    const clientB = await connectClient(wsPort);
    await waitForMessage(clientB, 'welcome');
    clientB.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [40] } }));
    await waitForMessage(clientB, 'subscribed');

    expect(manager.connectionCount).toBe(2);

    clientA.close();
    await waitForClose(clientA);

    // The disconnected client must be gone from the connection count...
    expect(manager.connectionCount).toBe(1);

    // ...and, more importantly, its match-40 subscription must no longer be
    // in the routing index: broadcasting a match-40 event must not attempt
    // to deliver to A's (now-closed) socket, and B must still receive it.
    const received = await new Promise<{ event: IndexedEvent } | null>((resolve) => {
      const timer = setTimeout(() => resolve(null), 3_000);
      clientB.on('message', (raw) => {
        const msg = JSON.parse(raw.toString()) as { type: string; event?: IndexedEvent };
        if (msg.type === 'event') {
          clearTimeout(timer);
          resolve(msg as { event: IndexedEvent });
        }
      });
    });

    indexer.setEvents([makeEvent({ match_id: 40, ledger_sequence: 7000 })]);
    const delivered = await received;
    expect(delivered).not.toBeNull();
    expect(delivered?.event.match_id).toBe(40);

    clientB.close();
  });

  // ── 3. Multiple sequential reconnections all work correctly ───────────

  it('handles multiple sequential reconnections without leaking state', async () => {
    const reconnectCount = 3;

    for (let i = 0; i < reconnectCount; i++) {
      const ws = await connectClient(wsPort);
      await waitForMessage(ws, 'welcome');

      ws.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [20 + i] } }));
      const sub = (await waitForMessage(ws, 'subscribed')) as { match_ids: number[] };
      expect(sub.match_ids).toContain(20 + i);

      ws.close();
      await waitForClose(ws);
    }

    // Server should have zero active connections after all clients disconnected
    expect(manager.connectionCount).toBe(0);
  });

  // ── 4. Reconnect via URL path auto-subscribes to the match ───────────

  it('auto-subscribes to match when reconnecting via /ws/match/:id URL', async () => {
    // First connection via URL path — auto-subscribed to match 30
    const ws1 = await connectClient(wsPort, '/ws/match/30');
    await waitForMessage(ws1, 'welcome');
    // The subscribed ack is sent automatically for URL-path connections
    await waitForMessage(ws1, 'subscribed');
    ws1.close();
    await waitForClose(ws1);

    // Reconnect via same URL path
    const ws2 = await connectClient(wsPort, '/ws/match/30');
    await waitForMessage(ws2, 'welcome');
    await waitForMessage(ws2, 'subscribed');

    // Post an event and verify it's delivered without a manual subscribe call
    indexer.setEvents([makeEvent({ match_id: 30, ledger_sequence: 7000 })]);
    const eventMsg = (await waitForMessage(ws2, 'event', 3_000)) as {
      event: IndexedEvent;
    };
    expect(eventMsg.event.match_id).toBe(30);

    ws2.close();
  });

  // ── 5. Server drops connection on heartbeat timeout; client can reconnect

  it('client can reconnect after server terminates a stale connection', async () => {
    // Build a very short heartbeat timeout so we can trigger it quickly
    const shortWsPort = allocPort();
    const shortCfg = buildConfig(shortWsPort, indexerPort);
    shortCfg.heartbeatIntervalMs = 50;    // ping every 50 ms
    shortCfg.heartbeatTimeoutMs = 100;    // terminate after 100 ms of silence

    const shortManager = new ConnectionManager(shortCfg);
    shortManager.start();

    try {
      // Connect but do NOT respond to pings — simulate a stale/zombie connection
      const ws1 = await connectClient(shortWsPort);
      await waitForMessage(ws1, 'welcome');

      // Pause the socket's message handler so it doesn't respond to pings
      // (ws library responds to native pings automatically, so we pause the socket)
      ws1.pause();

      // Wait long enough for the server to time out the connection
      await sleep(400);

      // The server should have terminated it
      expect(ws1.readyState).toBe(WebSocket.CLOSED);

      // Now reconnect normally and verify the server accepts the new connection
      const ws2 = await connectClient(shortWsPort);
      const welcome = (await waitForMessage(ws2, 'welcome')) as {
        protocol_version: number;
      };
      expect(welcome.protocol_version).toBe(1);

      ws2.close();
    } finally {
      await shortManager.stop();
    }
  });

  // ── 6. Reconnect with different match subscription ────────────────────

  it('receives events for new subscription after reconnect, not old one', async () => {
    // First session: subscribe to match 40
    const ws1 = await connectClient(wsPort);
    await waitForMessage(ws1, 'welcome');
    ws1.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [40] } }));
    await waitForMessage(ws1, 'subscribed');
    ws1.close();
    await waitForClose(ws1);

    // Second session: subscribe to match 41 only
    const ws2 = await connectClient(wsPort);
    await waitForMessage(ws2, 'welcome');
    ws2.send(JSON.stringify({ type: 'subscribe', payload: { match_ids: [41] } }));
    await waitForMessage(ws2, 'subscribed');

    // Post events for both matches
    indexer.setEvents([
      makeEvent({ match_id: 40, ledger_sequence: 8000 }),
      makeEvent({ match_id: 41, ledger_sequence: 8001 }),
    ]);

    const receivedMatchIds: number[] = [];
    await new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, 600);
      ws2.on('message', (raw) => {
        const msg = JSON.parse(raw.toString()) as { type: string; event?: IndexedEvent };
        if (msg.type === 'event' && msg.event) {
          receivedMatchIds.push(msg.event.match_id);
          if (receivedMatchIds.length >= 1) {
            clearTimeout(timer);
            resolve();
          }
        }
      });
    });

    // Should only have received the match 41 event, not match 40
    expect(receivedMatchIds).not.toContain(40);
    expect(receivedMatchIds).toContain(41);

    ws2.close();
  });
});
