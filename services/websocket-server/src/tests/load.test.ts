/**
 * Load test: 1000+ concurrent WebSocket connections
 *
 * Run with: npm run test:load
 * (This file is excluded from the normal vitest run because it starts a real
 * server and holds 1000+ sockets open simultaneously.)
 *
 * What it tests:
 *   1. The server accepts 1000 concurrent connections without crashing
 *   2. Every connected client receives the correct 'welcome' message
 *   3. Targeted event broadcast reaches only subscribed clients and not others
 *   4. Peak RSS memory is below a generous 512 MB threshold
 *   5. All connections close cleanly
 */

import WebSocket from 'ws';
import { ConnectionManager } from '../connectionManager.js';
import type { ServerConfig } from '../types.js';

const LOAD_PORT = 9099;
const TOTAL_CLIENTS = 1_000;
const SUBSCRIBED_MATCH_ID = 42;
const SUBSCRIBED_COUNT = 200; // first 200 clients subscribe to match 42

const TEST_CONFIG: ServerConfig = {
  port: LOAD_PORT,
  host: '127.0.0.1',
  eventIndexerUrl: 'http://localhost:8080',
  pollIntervalMs: 5_000,
  heartbeatIntervalMs: 60_000,    // long — don't interfere with the test
  heartbeatTimeoutMs: 120_000,
  rateLimitMaxSubscribes: 1_000,  // generous for load test
  rateLimitWindowMs: 60_000,
  maxSubscriptionsPerClient: 50,
  logLevel: 'warn',               // suppress noise during load test
};

// ─── Utilities ─────────────────────────────────────────────────────────────

function connectClient(url: string): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.once('open', () => resolve(ws));
    ws.once('error', reject);
  });
}

function waitForMessage(ws: WebSocket, type: string, timeoutMs = 5_000): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`Timed out waiting for '${type}'`)), timeoutMs);
    ws.on('message', function handler(raw) {
      try {
        const msg = JSON.parse(raw.toString()) as { type: string };
        if (msg.type === type) {
          clearTimeout(timer);
          ws.off('message', handler);
          resolve(msg);
        }
      } catch {/* ignore parse errors */}
    });
  });
}

function closeAll(sockets: WebSocket[]): Promise<void> {
  return new Promise((resolve) => {
    let pending = sockets.length;
    if (pending === 0) { resolve(); return; }
    for (const ws of sockets) {
      ws.once('close', () => { if (--pending === 0) resolve(); });
      ws.close();
    }
  });
}

// ─── Main ──────────────────────────────────────────────────────────────────

async function main() {
  const manager = new ConnectionManager(TEST_CONFIG);
  manager.start();

  const url = `ws://127.0.0.1:${LOAD_PORT}`;
  const sockets: WebSocket[] = [];

  try {
    console.log(`\nConnecting ${TOTAL_CLIENTS} clients to ${url}…`);
    const t0 = Date.now();

    // Connect all clients in batches of 100 to avoid overwhelming the OS
    const BATCH = 100;
    for (let i = 0; i < TOTAL_CLIENTS; i += BATCH) {
      const batch = Array.from({ length: Math.min(BATCH, TOTAL_CLIENTS - i) }, () =>
        connectClient(url),
      );
      sockets.push(...(await Promise.all(batch)));
    }

    const connectMs = Date.now() - t0;
    console.log(`✓ All ${TOTAL_CLIENTS} clients connected in ${connectMs}ms`);

    // Wait for welcome on all clients
    console.log('Waiting for welcome messages…');
    const welcomePromises = sockets.map((ws) => waitForMessage(ws, 'welcome', 10_000));
    await Promise.all(welcomePromises);
    console.log(`✓ All ${TOTAL_CLIENTS} clients received 'welcome'`);

    // Subscribe the first SUBSCRIBED_COUNT clients to match 42
    console.log(`Subscribing first ${SUBSCRIBED_COUNT} clients to match ${SUBSCRIBED_MATCH_ID}…`);
    const subMsg = JSON.stringify({
      type: 'subscribe',
      payload: { match_ids: [SUBSCRIBED_MATCH_ID], player_addresses: [] },
    });
    const subscribedSockets = sockets.slice(0, SUBSCRIBED_COUNT);
    const unsubscribedSockets = sockets.slice(SUBSCRIBED_COUNT);

    const subscribedPromises = subscribedSockets.map((ws) => {
      const p = waitForMessage(ws, 'subscribed', 10_000);
      ws.send(subMsg);
      return p;
    });
    await Promise.all(subscribedPromises);
    console.log(`✓ ${SUBSCRIBED_COUNT} clients subscribed`);

    // Broadcast an event — only subscribed clients should receive it
    console.log('Broadcasting event to subscribed clients…');
    const receivedBySubscribed: number[] = [];
    const receivedByUnsubscribed: number[] = [];

    // Set up listeners before broadcast
    const eventListeners: Promise<void>[] = [];
    subscribedSockets.forEach((ws, i) => {
      eventListeners.push(
        waitForMessage(ws, 'event', 5_000)
          .then(() => { receivedBySubscribed.push(i); })
          .catch(() => {/* timeout = did not receive */}),
      );
    });
    unsubscribedSockets.forEach((ws, i) => {
      // These should NOT receive an event; we give them 1 s to NOT fire
      eventListeners.push(
        new Promise<void>((resolve) => {
          const timer = setTimeout(resolve, 1_000);
          ws.once('message', (raw) => {
            try {
              const msg = JSON.parse(raw.toString()) as { type: string };
              if (msg.type === 'event') {
                receivedByUnsubscribed.push(i);
              }
            } catch {/* ignore */}
            clearTimeout(timer);
            resolve();
          });
        }),
      );
    });

    manager.broadcast({
      id: 'load-test-event',
      ledger_sequence: 999,
      match_id: SUBSCRIBED_MATCH_ID,
      event_type: 'match/created',
      player1: 'GABC',
      player2: 'GDEF',
      timestamp: new Date().toISOString(),
    });

    await Promise.all(eventListeners);

    // ── Assertions ──────────────────────────────────────────────────────
    let passed = true;

    if (manager.connectionCount !== TOTAL_CLIENTS) {
      console.error(`✗ Expected ${TOTAL_CLIENTS} connections, got ${manager.connectionCount}`);
      passed = false;
    } else {
      console.log(`✓ Server holds ${manager.connectionCount} concurrent connections`);
    }

    if (receivedBySubscribed.length < SUBSCRIBED_COUNT * 0.99) {
      // Allow ≤1% delivery failure (timing edge cases in test harness)
      console.error(
        `✗ Only ${receivedBySubscribed.length}/${SUBSCRIBED_COUNT} subscribed clients received the event`,
      );
      passed = false;
    } else {
      console.log(
        `✓ ${receivedBySubscribed.length}/${SUBSCRIBED_COUNT} subscribed clients received the event`,
      );
    }

    if (receivedByUnsubscribed.length > 0) {
      console.error(
        `✗ ${receivedByUnsubscribed.length} non-subscribed clients received the event (data leak!)`,
      );
      passed = false;
    } else {
      console.log('✓ Non-subscribed clients received no event (no data leak)');
    }

    const rssBytes = process.memoryUsage().rss;
    const rssMb = rssBytes / 1024 / 1024;
    const memLimit = 512;
    if (rssMb > memLimit) {
      console.error(`✗ RSS ${rssMb.toFixed(1)} MB exceeds ${memLimit} MB limit`);
      passed = false;
    } else {
      console.log(`✓ Peak RSS: ${rssMb.toFixed(1)} MB (limit: ${memLimit} MB)`);
    }

    // ── Cleanup ────────────────────────────────────────────────────────
    console.log('Closing all client connections…');
    const t1 = Date.now();
    await closeAll(sockets);
    console.log(`✓ All connections closed in ${Date.now() - t1}ms`);

    await manager.stop();

    if (!passed) {
      console.error('\n❌ Load test FAILED');
      process.exit(1);
    }

    console.log('\n✅ Load test PASSED');
    process.exit(0);
  } catch (err) {
    console.error('Load test error:', err);
    await closeAll(sockets).catch(() => {/* ignore */});
    await manager.stop().catch(() => {/* ignore */});
    process.exit(1);
  }
}

void main();
