/**
 * EventPoller pagination tests
 *
 * These tests verify that EventPoller correctly handles pagination when the
 * event-indexer contains more than 100 events (the default page size).
 *
 * Tests:
 * - EventPoller pages through results using limit/offset query params
 * - All events are eventually fetched despite pagination
 * - No events are missed or duplicated when >100 events exist
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import http from 'http';
import { EventPoller } from '../eventPoller.js';
import type { IndexedEvent, ServerConfig } from '../types.js';

// ─── Port registry ────────────────────────────────────────────────

let nextPort = 9300;
function allocPort(): number { return nextPort++; }

// ─── Helpers ──────────────────────────────────────────────────────

function buildConfig(indexerPort: number): Pick<ServerConfig, 'eventIndexerUrl' | 'pollIntervalMs'> {
  return {
    eventIndexerUrl: `http://127.0.0.1:${indexerPort}`,
    pollIntervalMs: 100,
  };
}

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: Math.floor(Math.random() * 1_000_000).toString(),
    ledger_sequence: 1,
    match_id: 1n,
    event_type: 'contract_event',
    player1: 'player1',
    player2: 'player2',
    status: 'active',
    winner: null,
    stake_amount: '100',
    token: 'native',
    game_id: 'game1',
    platform: 'test',
    timestamp: new Date().toISOString(),
    txn_hash: 'hash',
    event_index_in_txn: 0,
    ...overrides,
  };
}

/**
 * Creates a mock indexer that supports offset-based pagination via
 * `limit` and `after_index` query params.
 *
 * The mock tracks state:
 * - `allEvents`: the complete list of events to paginate through
 * - `pageSize`: events per page (default 100)
 * - Implements GET /events with optional ?limit=<n>&offset=<n> query params
 */
function createPaginatedMockIndexer(
  port: number,
  pageSize: number = 100,
): {
  setAllEvents: (events: IndexedEvent[]) => void;
  stop: () => Promise<void>;
} {
  let allEvents: IndexedEvent[] = [];
  const server = http.createServer((req, res) => {
    const url = new URL(req.url || '/', `http://localhost:${port}`);
    const limit = parseInt(url.searchParams.get('limit') ?? String(pageSize), 10);
    const offset = parseInt(url.searchParams.get('offset') ?? '0', 10);

    // Sort events by ledger, then index
    const sorted = [...allEvents].sort((a, b) => {
      if (a.ledger_sequence !== b.ledger_sequence) {
        return a.ledger_sequence - b.ledger_sequence;
      }
      return (a.event_index_in_txn || 0) - (b.event_index_in_txn || 0);
    });

    // Paginate using limit/offset
    const page = sorted.slice(offset, offset + limit);

    if (page.length === 0) {
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ success: false, data: null, error: 'No events found' }));
    } else {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ success: true, data: page, error: null }));
    }
  });

  server.listen(port);

  return {
    setAllEvents: (events) => { allEvents = events; },
    stop: () =>
      new Promise((resolve, reject) =>
        server.close((err) => (err ? reject(err) : resolve())),
      ),
  };
}

describe('EventPoller pagination (>100 events)', () => {
  it('fetches all events when total count exceeds default page size', async () => {
    const indexerPort = allocPort();
    const indexer = createPaginatedMockIndexer(indexerPort, 100);
    const config = buildConfig(indexerPort);

    // Create 250 events spread across ledgers
    const allEvents: IndexedEvent[] = [];
    for (let ledger = 1; ledger <= 10; ledger++) {
      for (let i = 0; i < 25; i++) {
        allEvents.push(makeEvent({
          ledger_sequence: ledger,
          event_index_in_txn: i,
          id: `event-${ledger}-${i}`,
        }));
      }
    }

    indexer.setAllEvents(allEvents);

    // Collect events from EventPoller
    const collectedEvents: IndexedEvent[] = [];
    let pollerError: Error | null = null;

    const poller = new EventPoller(config, (event) => {
      collectedEvents.push(event);
    });

    poller.start();

    // Wait for polling to complete (with timeout)
    await new Promise<void>((resolve) => {
      const timeoutHandle = setTimeout(() => {
        poller.stop();
        resolve();
      }, 2000);

      // Poll until we've collected all events or timeout
      const checkHandle = setInterval(() => {
        if (collectedEvents.length >= allEvents.length) {
          clearTimeout(timeoutHandle);
          clearInterval(checkHandle);
          poller.stop();
          resolve();
        }
      }, 50);
    });

    await indexer.stop();

    // Verify: all events were fetched
    expect(collectedEvents.length).toBe(allEvents.length);

    // Verify: no duplicates
    const ids = new Set(collectedEvents.map((e) => e.id));
    expect(ids.size).toBe(collectedEvents.length);

    // Verify: events are in ledger order
    for (let i = 1; i < collectedEvents.length; i++) {
      const prev = collectedEvents[i - 1];
      const curr = collectedEvents[i];
      const ledgerOrder = curr.ledger_sequence >= prev.ledger_sequence;
      if (curr.ledger_sequence === prev.ledger_sequence) {
        const indexOrder = (curr.event_index_in_txn || 0) >= (prev.event_index_in_txn || 0);
        expect(indexOrder).toBe(true);
      } else {
        expect(ledgerOrder).toBe(true);
      }
    }
  }, { timeout: 10000 });

  it('updates watermark correctly between pages', async () => {
    const indexerPort = allocPort();
    const indexer = createPaginatedMockIndexer(indexerPort, 50); // Small page size
    const config = buildConfig(indexerPort);

    // Create 150 events
    const allEvents: IndexedEvent[] = [];
    for (let i = 0; i < 150; i++) {
      allEvents.push(makeEvent({
        ledger_sequence: Math.floor(i / 10) + 1,
        event_index_in_txn: i % 10,
        id: `event-${i}`,
      }));
    }

    indexer.setAllEvents(allEvents);

    const collectedEvents: IndexedEvent[] = [];
    const poller = new EventPoller(config, (event) => {
      collectedEvents.push(event);
    });

    poller.start();

    // Wait for polling
    await new Promise<void>((resolve) => {
      const timeoutHandle = setTimeout(() => {
        poller.stop();
        resolve();
      }, 3000);

      const checkHandle = setInterval(() => {
        if (collectedEvents.length >= allEvents.length) {
          clearTimeout(timeoutHandle);
          clearInterval(checkHandle);
          poller.stop();
          resolve();
        }
      }, 50);
    });

    await indexer.stop();

    // Verify all events collected
    expect(collectedEvents.length).toBe(allEvents.length);
  }, { timeout: 10000 });

  it('handles multiple pages correctly with offset pagination', async () => {
    const indexerPort = allocPort();
    const indexer = createPaginatedMockIndexer(indexerPort, 30); // Very small page size
    const config = buildConfig(indexerPort);

    // Create 120 events (4 pages of 30)
    const allEvents: IndexedEvent[] = [];
    for (let i = 0; i < 120; i++) {
      allEvents.push(makeEvent({
        ledger_sequence: Math.floor(i / 30) + 1,
        event_index_in_txn: i % 30,
        id: `event-${i}`,
      }));
    }

    indexer.setAllEvents(allEvents);

    const collectedEvents: IndexedEvent[] = [];
    const poller = new EventPoller(config, (event) => {
      collectedEvents.push(event);
    });

    poller.start();

    // Wait for polling
    await new Promise<void>((resolve) => {
      const timeoutHandle = setTimeout(() => {
        poller.stop();
        resolve();
      }, 3000);

      const checkHandle = setInterval(() => {
        if (collectedEvents.length >= allEvents.length) {
          clearTimeout(timeoutHandle);
          clearInterval(checkHandle);
          poller.stop();
          resolve();
        }
      }, 50);
    });

    await indexer.stop();

    // Verify all 120 events were collected across 4 pages
    expect(collectedEvents.length).toBe(allEvents.length);

    // Verify no duplicate event IDs
    const idSet = new Set(collectedEvents.map((e) => e.id));
    expect(idSet.size).toBe(collectedEvents.length);
  }, { timeout: 10000 });
});
