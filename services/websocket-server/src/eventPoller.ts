/**
 * EventPoller
 *
 * Periodically fetches new events from the event-indexer REST API and calls a
 * callback for each one.  Tracks the highest seen ledger sequence so every
 * event is emitted exactly once.
 *
 * Uses offset pagination (limit/offset query params) to fetch all pages in a
 * single poll cycle, then deduplicates via the watermark.
 *
 * Retry strategy: exponential back-off (1 s → 2 s → 4 s … capped at 30 s)
 * with jitter so multiple instances don't thundering-herd the indexer.
 */

import type { IndexedEvent, ServerConfig } from './types.js';
import { logger } from './logger.js';

type EventCallback = (event: IndexedEvent) => void;

interface ApiResponse<T> {
  success: boolean;
  data: T | null;
  error: string | null;
}

/** Page size sent to the event-indexer /events endpoint. */
const PAGE_SIZE = 100;

export class EventPoller {
  private running = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** Watermark: (ledger_sequence, event_index_in_txn) of last dispatched event */
  private highWatermark: { ledger: number; index: number } = { ledger: 0, index: -1 };
  /** Consecutive failure count (for back-off) */
  private consecutiveFailures = 0;

  constructor(
    private readonly config: Pick<ServerConfig, 'eventIndexerUrl' | 'pollIntervalMs'>,
    private readonly onEvent: EventCallback,
  ) {}

  async start(): Promise<void> {
    if (this.running) return;
    this.running = true;
    await this.initializeWatermark();
    this.scheduleNext(0); // first poll immediately
  }

  stop(): void {
    this.running = false;
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  // ─── Internal ──────────────────────────────────────────

  private async initializeWatermark(): Promise<void> {
    try {
      const events = await this.fetchNewEvents();
      if (events.length > 0) {
        const lastEvent = events[events.length - 1]!;
        this.highWatermark = {
          ledger: lastEvent.ledger_sequence,
          index: lastEvent.event_index_in_txn ?? 0,
        };
        logger.info(
          { ledger: this.highWatermark.ledger, index: this.highWatermark.index },
          'EventPoller initialized watermark from latest indexed ledger',
        );
      }
    } catch (err) {
      logger.warn(
        { err },
        'EventPoller failed to initialize watermark; will start from beginning',
      );
    }
  }

  private scheduleNext(delayMs: number): void {
    this.timer = setTimeout(() => {
      void this.poll();
    }, delayMs);
  }

  private async poll(): Promise<void> {
    if (!this.running) return;

    try {
      const events = await this.fetchNewEvents();
      this.consecutiveFailures = 0;

      let maxWatermark = { ...this.highWatermark };
      // Sort ascending so callbacks arrive in ledger order
      const sorted = events.sort(
        (a, b) => a.ledger_sequence - b.ledger_sequence || (a.event_index_in_txn ?? 0) - (b.event_index_in_txn ?? 0),
      );

      for (const event of sorted) {
        const eventIndex = event.event_index_in_txn ?? 0;
        const isNewEvent =
          event.ledger_sequence > this.highWatermark.ledger ||
          (event.ledger_sequence === this.highWatermark.ledger && eventIndex > this.highWatermark.index);

        if (isNewEvent) {
          this.onEvent(event);
          if (event.ledger_sequence > maxWatermark.ledger ||
              (event.ledger_sequence === maxWatermark.ledger && eventIndex > maxWatermark.index)) {
            maxWatermark = { ledger: event.ledger_sequence, index: eventIndex };
          }
        }
      }

      this.highWatermark = maxWatermark;
      this.scheduleNext(this.config.pollIntervalMs);
    } catch (err) {
      this.consecutiveFailures += 1;
      const backoff = Math.min(1000 * 2 ** (this.consecutiveFailures - 1), 30_000);
      const jitter = Math.random() * 500;
      const delay = backoff + jitter;
      logger.warn(
        { err, consecutiveFailures: this.consecutiveFailures, retryAfterMs: Math.round(delay) },
        'EventPoller: fetch failed, retrying with back-off',
      );
      this.scheduleNext(delay);
    }
  }

  /**
   * Fetch all new events from the indexer, paginating through results
   * using limit/offset query params until every page is retrieved.
   */
  private async fetchNewEvents(): Promise<IndexedEvent[]> {
    const allEvents: IndexedEvent[] = [];
    let offset = 0;

    while (true) {
      const url = `${this.config.eventIndexerUrl}/events?limit=${PAGE_SIZE}&offset=${offset}`;
      const res = await fetch(url);
      if (!res.ok) {
        throw new Error(`Event-indexer responded ${res.status} ${res.statusText}`);
      }
      const body = (await res.json()) as ApiResponse<IndexedEvent[]>;
      if (!body.success || !body.data || body.data.length === 0) {
        break;
      }
      allEvents.push(...body.data);
      if (body.data.length < PAGE_SIZE) {
        break;
      }
      offset += PAGE_SIZE;
    }

    return allEvents;
  }
}
