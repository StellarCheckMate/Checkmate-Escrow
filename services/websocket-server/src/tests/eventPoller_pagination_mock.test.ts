/**
 * EventPoller pagination tests using a mocked `fetch`.
 *
 * These tests verify that EventPoller correctly handles paged indexer
 * responses without spinning up a real HTTP server.
 *
 * Coverage:
 *  - >100 events are fetched across multiple pages and emitted exactly once
 *  - Events split across polls in the same ledger are all emitted in order
 *  - Restart behaviour: watermark is initialised and no historical events are replayed
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { EventPoller } from '../eventPoller.js';
import type { IndexedEvent, ServerConfig } from '../types.js';

// ─── Helpers ──────────────────────────────────────────────────────

function buildConfig(): Pick<ServerConfig, 'eventIndexerUrl' | 'pollIntervalMs'> {
  return {
    eventIndexerUrl: 'http://localhost:8080',
    pollIntervalMs: 100,
  };
}

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: `evt-${Date.now()}-${Math.random()}`,
    ledger_sequence: 1,
    match_id: 1,
    event_type: 'match/created',
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
 * Build a mock `fetch` that returns paged responses.
 *
 * @param allEvents   - the complete ordered list of events the indexer holds
 * @param pageSize    - how many events each page returns (default 100)
 * @returns a `vi.fn` that can be passed to `global.fetch`, plus a helper
 *          to update the event list mid-test.
 */
function createPagedFetchMock(
  allEvents: IndexedEvent[],
  pageSize: number = 100,
): {
  fetchMock: ReturnType<typeof vi.fn>;
  setEvents: (events: IndexedEvent[]) => void;
} {
  let events = [...allEvents];

  const fetchMock = vi.fn(async (url: string) => {
    const parsed = new URL(url, 'http://localhost');
    const limit = parseInt(parsed.searchParams.get('limit') ?? String(pageSize), 10);
    const offset = parseInt(parsed.searchParams.get('offset') ?? '0', 10);

    const page = events.slice(offset, offset + limit);

    if (page.length === 0) {
      return new Response(
        JSON.stringify({ success: false, data: null, error: 'No events found' }),
        { status: 404, headers: { 'Content-Type': 'application/json' } },
      );
    }

    return new Response(
      JSON.stringify({ success: true, data: page, error: null }),
      { status: 200, headers: { 'Content-Type': 'application/json' } },
    );
  });

  return {
    fetchMock,
    setEvents: (newEvents: IndexedEvent[]) => { events = [...newEvents]; },
  };
}

// ─── Test suite ───────────────────────────────────────────────────

describe('EventPoller pagination with mocked fetch', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let setEvents: (events: IndexedEvent[]) => void;

  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const mock = createPagedFetchMock([]);
    fetchMock = mock.fetchMock;
    setEvents = mock.setEvents;
    global.fetch = fetchMock;
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  // ── >100 events (paged responses) ──────────────────────────

  it('fetches all events across multiple pages and emits each exactly once', async () => {
    const TOTAL = 250;
    const events: IndexedEvent[] = [];
    for (let i = 0; i < TOTAL; i++) {
      events.push(makeEvent({
        ledger_sequence: Math.floor(i / 10) + 1,
        event_index_in_txn: i % 10,
        id: `event-${i}`,
      }));
    }
    setEvents(events);

    const collected: IndexedEvent[] = [];
    const poller = new EventPoller(buildConfig(), (e) => collected.push(e));
    await poller.start();

    // Advance enough for the initial poll + all pagination requests to complete
    await vi.advanceTimersByTimeAsync(500);

    poller.stop();

    // Every event must be emitted exactly once
    expect(collected.length).toBe(TOTAL);
    const ids = collected.map((e) => e.id);
    expect(new Set(ids).size).toBe(TOTAL);

    // Events must arrive in ledger order (then index order)
    for (let i = 1; i < collected.length; i++) {
      const prev = collected[i - 1]!;
      const curr = collected[i]!;
      expect(curr.ledger_sequence).toBeGreaterThanOrEqual(prev.ledger_sequence);
      if (curr.ledger_sequence === prev.ledger_sequence) {
        expect(curr.event_index_in_txn ?? 0).toBeGreaterThanOrEqual(prev.event_index_in_txn ?? 0);
      }
    }
  });

  it('requests every page when total exceeds page size', async () => {
    const PAGE_SIZE = 100;
    const TOTAL = 250; // 3 pages: 100 + 100 + 50
    const events: IndexedEvent[] = [];
    for (let i = 0; i < TOTAL; i++) {
      events.push(makeEvent({
        ledger_sequence: 1,
        event_index_in_txn: i,
        id: `event-${i}`,
      }));
    }
    setEvents(events);

    const poller = new EventPoller(buildConfig(), () => {});
    await poller.start();
    await vi.advanceTimersByTimeAsync(500);
    poller.stop();

    // fetch was called once per page + once for the final empty page
    // Page 1 (offset=0), Page 2 (offset=100), Page 3 (offset=200), Page 4 (offset=300 → empty → stop)
    const eventUrls = fetchMock.mock.calls
      .filter((call: unknown[]) => typeof call[0] === 'string' && call[0].includes('/events'))
      .map((call: unknown[]) => call[0] as string);

    expect(eventUrls.length).toBeGreaterThanOrEqual(3);

    // Verify offset params increase by PAGE_SIZE
    const offsets = eventUrls.map((url: string) => {
      const u = new URL(url, 'http://localhost');
      return parseInt(u.searchParams.get('offset') ?? '0', 10);
    });
    expect(offsets).toContain(0);
    expect(offsets).toContain(PAGE_SIZE);
    expect(offsets).toContain(PAGE_SIZE * 2);
  });

  // ── Same-ledger events split across polls ──────────────────

  it('emits all same-ledger events that arrive in separate polls', async () => {
    const events: IndexedEvent[] = [];
    for (let i = 0; i < 5; i++) {
      events.push(makeEvent({
        ledger_sequence: 100,
        event_index_in_txn: i,
        id: `same-ledger-${i}`,
      }));
    }
    setEvents(events);

    const collected: IndexedEvent[] = [];
    const poller = new EventPoller(buildConfig(), (e) => collected.push(e));
    await poller.start();

    // First poll collects all 5 events (they're all new)
    await vi.advanceTimersByTimeAsync(200);

    // Simulate a new event arriving in the same ledger on the next poll
    setEvents([
      ...events,
      makeEvent({ ledger_sequence: 100, event_index_in_txn: 5, id: 'same-ledger-5' }),
    ]);

    await vi.advanceTimersByTimeAsync(200);

    poller.stop();

    // All 6 events from ledger 100 must be emitted exactly once
    expect(collected.length).toBe(6);
    const ids = collected.map((e) => e.id);
    expect(new Set(ids).size).toBe(6);
    expect(ids).toContain('same-ledger-5');
  });

  it('does not drop events when multiple events share the same ledger across polls', async () => {
    // First poll: 2 events from ledger 200
    setEvents([
      makeEvent({ ledger_sequence: 200, event_index_in_txn: 0, id: 'a' }),
      makeEvent({ ledger_sequence: 200, event_index_in_txn: 1, id: 'b' }),
    ]);

    const collected: IndexedEvent[] = [];
    const poller = new EventPoller(buildConfig(), (e) => collected.push(e));
    await poller.start();
    await vi.advanceTimersByTimeAsync(200);

    // Second poll: 3 events from ledger 200 (2 old + 1 new)
    setEvents([
      makeEvent({ ledger_sequence: 200, event_index_in_txn: 0, id: 'a' }),
      makeEvent({ ledger_sequence: 200, event_index_in_txn: 1, id: 'b' }),
      makeEvent({ ledger_sequence: 200, event_index_in_txn: 2, id: 'c' }),
    ]);

    await vi.advanceTimersByTimeAsync(200);

    poller.stop();

    // Must have all 3 events, no duplicates
    expect(collected.map((e) => e.id)).toEqual(['a', 'b', 'c']);
  });

  // ── Restart behaviour ──────────────────────────────────────

  it('initialises watermark from latest indexed ledger on start', async () => {
    // Pre-populate indexer with historical events
    setEvents([
      makeEvent({ ledger_sequence: 10, event_index_in_txn: 0, id: 'old-1' }),
      makeEvent({ ledger_sequence: 10, event_index_in_txn: 1, id: 'old-2' }),
      makeEvent({ ledger_sequence: 11, event_index_in_txn: 0, id: 'old-3' }),
    ]);

    const collected: IndexedEvent[] = [];
    const poller = new EventPoller(buildConfig(), (e) => collected.push(e));
    await poller.start();

    // Historical events should not be emitted after initialisation
    await vi.advanceTimersByTimeAsync(200);
    expect(collected).toHaveLength(0);

    poller.stop();
  });

  it('does not replay historical events after restart', async () => {
    const collected: IndexedEvent[] = [];
    const poller = new EventPoller(buildConfig(), (e) => collected.push(e));

    // First run: populate with events and start
    setEvents([
      makeEvent({ ledger_sequence: 50, event_index_in_txn: 0, id: 'evt-1' }),
      makeEvent({ ledger_sequence: 50, event_index_in_txn: 1, id: 'evt-2' }),
    ]);
    await poller.start();
    await vi.advanceTimersByTimeAsync(200);
    poller.stop();

    // Restart: same events should not be replayed
    collected.length = 0;
    await poller.start();
    await vi.advanceTimersByTimeAsync(200);

    expect(collected).toHaveLength(0);

    // New events should be emitted
    setEvents([
      makeEvent({ ledger_sequence: 50, event_index_in_txn: 0, id: 'evt-1' }),
      makeEvent({ ledger_sequence: 50, event_index_in_txn: 1, id: 'evt-2' }),
      makeEvent({ ledger_sequence: 51, event_index_in_txn: 0, id: 'evt-new' }),
    ]);

    await vi.advanceTimersByTimeAsync(200);
    poller.stop();

    expect(collected.map((e) => e.id)).toEqual(['evt-new']);
  });

  it('handles initialisation failure gracefully and starts from beginning', async () => {
    // Make fetch throw during initialisation
    const originalFetch = global.fetch;
    global.fetch = vi.fn(async () => {
      throw new Error('Network error during init');
    });

    const collected: IndexedEvent[] = [];
    const poller = new EventPoller(buildConfig(), (e) => collected.push(e));
    await poller.start();

    // Restore fetch and provide events
    const events = [makeEvent({ id: 'after-failure' })];
    const mock = createPagedFetchMock(events);
    global.fetch = mock.fetchMock;

    await vi.advanceTimersByTimeAsync(200);
    poller.stop();

    // Should have recovered and emitted the event
    expect(collected.map((e) => e.id)).toContain('after-failure');

    global.fetch = originalFetch;
  });
});
