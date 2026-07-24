/**
 * SubscriptionManager
 *
 * Tracks which clients are interested in which match IDs / player addresses
 * and efficiently routes incoming events to the correct set of WebSocket
 * connections.
 *
 * Design decisions:
 *  - Two separate indices (matchId → clientIds, playerAddress → clientIds) for
 *    O(1) dispatch without iterating over every connected client.
 *  - All mutations are synchronous; the caller is responsible for concurrency.
 *  - Validation of subscription payloads happens here, not in the transport
 *    layer, so the same logic can be exercised in pure-unit tests.
 */

import type { ClientState, IndexedEvent, ServerConfig } from './types.js';

export interface SubscriptionResult {
  ok: boolean;
  error?: string;
}

export class SubscriptionManager {
  /** clientId → ClientState */
  private readonly clients = new Map<string, ClientState>();

  /** matchId → Set<clientId> */
  private readonly matchIndex = new Map<number, Set<string>>();

  /** playerAddress (lower-cased) → Set<clientId> */
  private readonly playerIndex = new Map<string, Set<string>>();

  constructor(private readonly config: Pick<ServerConfig, 'maxSubscriptionsPerClient' | 'rateLimitMaxSubscribes' | 'rateLimitWindowMs'>) {}

  // ─── Client lifecycle ──────────────────────────────────────────────────

  addClient(state: ClientState): void {
    this.clients.set(state.id, state);
  }

  removeClient(clientId: string): void {
    const state = this.clients.get(clientId);
    if (!state) return;

    // Remove from match index
    for (const matchId of state.matchIds) {
      this.removeFromMatchIndex(matchId, clientId);
    }

    // Remove from player index
    for (const addr of state.playerAddresses) {
      this.removeFromPlayerIndex(addr, clientId);
    }

    this.clients.delete(clientId);
  }

  getClient(clientId: string): ClientState | undefined {
    return this.clients.get(clientId);
  }

  get clientCount(): number {
    return this.clients.size;
  }

  allClients(): IterableIterator<ClientState> {
    return this.clients.values();
  }

  // ─── Subscription mutation ─────────────────────────────────────────────

  /**
   * Subscribe a client to a set of match IDs and/or player addresses.
   *
   * Enforces:
   *  - Rate limiting (max N subscribe commands per window)
   *  - Per-client subscription cap
   *  - Basic input validation
   */
  subscribe(
    clientId: string,
    matchIds: number[] = [],
    playerAddresses: string[] = [],
  ): SubscriptionResult {
    const state = this.clients.get(clientId);
    if (!state) return { ok: false, error: 'Client not found' };

    // ── Rate limit ────────────────────────────────────────────────────────
    const now = Date.now();
    const windowElapsed = now - state.rateLimitWindowStart;
    if (windowElapsed > this.config.rateLimitWindowMs) {
      state.subscribeCount = 0;
      state.rateLimitWindowStart = now;
    }
    state.subscribeCount += 1;
    if (state.subscribeCount > this.config.rateLimitMaxSubscribes) {
      return {
        ok: false,
        error: `rate_limited: max ${this.config.rateLimitMaxSubscribes} subscription commands per ${this.config.rateLimitWindowMs}ms`,
      };
    }

    // ── Input validation ──────────────────────────────────────────────────
    for (const id of matchIds) {
      if (!Number.isInteger(id) || id < 0) {
        return { ok: false, error: `Invalid match_id: ${id}` };
      }
    }
    for (const addr of playerAddresses) {
      if (typeof addr !== 'string' || addr.trim().length === 0) {
        return { ok: false, error: `Invalid player_address: ${String(addr)}` };
      }
    }

    // ── Cap check ─────────────────────────────────────────────────────────
    const newMatchIds = matchIds.filter((id) => !state.matchIds.has(id));
    const newAddresses = playerAddresses.filter((a) => !state.playerAddresses.has(a.toLowerCase()));
    const projectedTotal =
      state.matchIds.size + state.playerAddresses.size + newMatchIds.length + newAddresses.length;
    if (projectedTotal > this.config.maxSubscriptionsPerClient) {
      return {
        ok: false,
        error: `Subscription cap exceeded (max ${this.config.maxSubscriptionsPerClient})`,
      };
    }

    // ── Apply ─────────────────────────────────────────────────────────────
    for (const id of newMatchIds) {
      state.matchIds.add(id);
      let set = this.matchIndex.get(id);
      if (!set) {
        set = new Set();
        this.matchIndex.set(id, set);
      }
      set.add(clientId);
    }

    for (const raw of newAddresses) {
      const addr = raw.toLowerCase();
      state.playerAddresses.add(addr);
      let set = this.playerIndex.get(addr);
      if (!set) {
        set = new Set();
        this.playerIndex.set(addr, set);
      }
      set.add(clientId);
    }

    return { ok: true };
  }

  /**
   * Unsubscribe a client from the given match IDs and/or player addresses.
   */
  unsubscribe(
    clientId: string,
    matchIds: number[] = [],
    playerAddresses: string[] = [],
  ): SubscriptionResult {
    const state = this.clients.get(clientId);
    if (!state) return { ok: false, error: 'Client not found' };

    for (const id of matchIds) {
      state.matchIds.delete(id);
      this.removeFromMatchIndex(id, clientId);
    }

    for (const raw of playerAddresses) {
      const addr = raw.toLowerCase();
      state.playerAddresses.delete(addr);
      this.removeFromPlayerIndex(addr, clientId);
    }

    return { ok: true };
  }

  // ─── Event routing ─────────────────────────────────────────────────────

  /**
   * Given an event, returns the deduplicated set of client IDs that should
   * receive it based on their subscriptions.
   */
  matchingClients(event: IndexedEvent): Set<string> {
    const result = new Set<string>();

    // Match by match_id
    const byMatch = this.matchIndex.get(event.match_id);
    if (byMatch) {
      for (const id of byMatch) result.add(id);
    }

    // Match by player address (player1 or player2)
    if (event.player1) {
      const byPlayer1 = this.playerIndex.get(event.player1.toLowerCase());
      if (byPlayer1) {
        for (const id of byPlayer1) result.add(id);
      }
    }
    if (event.player2) {
      const byPlayer2 = this.playerIndex.get(event.player2.toLowerCase());
      if (byPlayer2) {
        for (const id of byPlayer2) result.add(id);
      }
    }

    return result;
  }

  // ─── Helpers ───────────────────────────────────────────────────────────

  private removeFromMatchIndex(matchId: number, clientId: string): void {
    const set = this.matchIndex.get(matchId);
    if (!set) return;
    set.delete(clientId);
    if (set.size === 0) this.matchIndex.delete(matchId);
  }

  private removeFromPlayerIndex(addr: string, clientId: string): void {
    const set = this.playerIndex.get(addr);
    if (!set) return;
    set.delete(clientId);
    if (set.size === 0) this.playerIndex.delete(addr);
  }
}
