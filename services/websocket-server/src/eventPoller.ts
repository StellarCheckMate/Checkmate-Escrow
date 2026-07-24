/**
 * EventPoller
 *
 * Periodically fetches new events from the event-indexer REST API and calls a
 * callback for each one.  Tracks the highest seen ledger sequence so every
 * event is emitted exactly once.
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

export class EventPoller {
  private running = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** Highest ledger_sequence we have already dispatched */
  private highWatermark = 0;
  /** Consecutive failure count (for back-off) */
  private consecutiveFailures = 0;

  constructor(
    private readonly config: Pick<ServerConfig, 'eventIndexerUrl' | 'pollIntervalMs'>,
    private readonly onEvent: EventCallback,
  ) {}

  start(): void {
    if (this.running) return;
    this.running = true;
    this.scheduleNext(0); // first poll immediately
  }

  stop(): void {
    this.running = false;
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  // ─── Internal ──────────────────────────────────────────────────────────

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

      let maxLedger = this.highWatermark;
      // Sort ascending so callbacks arrive in ledger order
      const sorted = events.sort(
        (a, b) => a.ledger_sequence - b.ledger_sequence || a.event_index_in_txn! - b.event_index_in_txn!,
      );

      for (const event of sorted) {
        if (event.ledger_sequence > this.highWatermark) {
          this.onEvent(event);
          if (event.ledger_sequence > maxLedger) maxLedger = event.ledger_sequence;
        }
      }

      this.highWatermark = maxLedger;
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

  private async fetchNewEvents(): Promise<IndexedEvent[]> {
    const url = `${this.config.eventIndexerUrl}/events`;
    const res = await fetch(url);
    if (!res.ok) {
      throw new Error(`Event-indexer responded ${res.status} ${res.statusText}`);
    }
    const body = (await res.json()) as ApiResponse<IndexedEvent[]>;
    if (!body.success || !body.data) {
      // The indexer returns 404 with success=false when no events exist yet
      return [];
    }
    return body.data;
  }
}
