/**
 * Unit tests for SubscriptionManager
 *
 * These tests exercise subscription filtering in isolation — no network I/O,
 * no WebSocket connection required.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { SubscriptionManager } from '../subscriptionManager.js';
import type { ClientState, IndexedEvent } from '../types.js';

// ─── Helpers ─────────────────────────────────────────────────────────────────

const DEFAULT_CONFIG = {
  maxSubscriptionsPerClient: 50,
  rateLimitMaxSubscribes: 20,
  rateLimitWindowMs: 60_000,
};

function makeState(overrides: Partial<ClientState> = {}): ClientState {
  return {
    id: Math.random().toString(36).slice(2),
    matchIds: new Set(),
    playerAddresses: new Set(),
    lastSeenAt: Date.now(),
    subscribeCount: 0,
    rateLimitWindowStart: Date.now(),
    remoteAddress: '127.0.0.1',
    ...overrides,
  };
}

function makeEvent(overrides: Partial<IndexedEvent> = {}): IndexedEvent {
  return {
    id: 'evt-1',
    ledger_sequence: 100,
    match_id: 1,
    event_type: 'match/created',
    player1: 'GABC',
    player2: 'GDEF',
    timestamp: new Date().toISOString(),
    ...overrides,
  };
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe('SubscriptionManager', () => {
  let manager: SubscriptionManager;

  beforeEach(() => {
    manager = new SubscriptionManager(DEFAULT_CONFIG);
  });

  // ── Client lifecycle ────────────────────────────────────────────────────

  describe('client lifecycle', () => {
    it('adds a client', () => {
      const state = makeState();
      manager.addClient(state);
      expect(manager.clientCount).toBe(1);
      expect(manager.getClient(state.id)).toBe(state);
    });

    it('removes a client and cleans up indices', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [42], ['GABC']);
      manager.removeClient(state.id);
      expect(manager.clientCount).toBe(0);
      expect(manager.getClient(state.id)).toBeUndefined();
    });

    it('is a no-op to remove a non-existent client', () => {
      expect(() => manager.removeClient('nonexistent')).not.toThrow();
    });
  });

  // ── Subscribe ───────────────────────────────────────────────────────────

  describe('subscribe', () => {
    it('subscribes to match IDs', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.subscribe(state.id, [1, 2, 3]);
      expect(result.ok).toBe(true);
      expect(state.matchIds.has(1)).toBe(true);
      expect(state.matchIds.has(2)).toBe(true);
      expect(state.matchIds.has(3)).toBe(true);
    });

    it('subscribes to player addresses (case-normalised)', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.subscribe(state.id, [], ['GABC', 'gDEF']);
      expect(result.ok).toBe(true);
      expect(state.playerAddresses.has('gabc')).toBe(true);
      expect(state.playerAddresses.has('gdef')).toBe(true);
    });

    it('is idempotent for duplicate subscriptions', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [1]);
      manager.subscribe(state.id, [1]);
      expect(state.matchIds.size).toBe(1);
    });

    it('rejects invalid match IDs (negative)', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.subscribe(state.id, [-1]);
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/Invalid match_id/);
    });

    it('rejects invalid match IDs (non-integer)', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.subscribe(state.id, [1.5 as unknown as number]);
      expect(result.ok).toBe(false);
    });

    it('rejects empty player address strings', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.subscribe(state.id, [], ['   ']);
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/Invalid player_address/);
    });

    it('enforces per-client subscription cap', () => {
      const mgr = new SubscriptionManager({ ...DEFAULT_CONFIG, maxSubscriptionsPerClient: 3 });
      const state = makeState();
      mgr.addClient(state);
      mgr.subscribe(state.id, [1, 2, 3]);
      const result = mgr.subscribe(state.id, [4]);
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/cap exceeded/);
    });

    it('returns error for unknown client ID', () => {
      const result = manager.subscribe('ghost', [1]);
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/Client not found/);
    });
  });

  // ── Unsubscribe ─────────────────────────────────────────────────────────

  describe('unsubscribe', () => {
    it('removes match IDs from subscription', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [1, 2]);
      manager.unsubscribe(state.id, [1]);
      expect(state.matchIds.has(1)).toBe(false);
      expect(state.matchIds.has(2)).toBe(true);
    });

    it('removes player addresses from subscription', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [], ['GABC', 'GDEF']);
      manager.unsubscribe(state.id, [], ['GABC']);
      expect(state.playerAddresses.has('gabc')).toBe(false);
      expect(state.playerAddresses.has('gdef')).toBe(true);
    });

    it('is a no-op for match IDs the client did not subscribe to', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.unsubscribe(state.id, [999]);
      expect(result.ok).toBe(true);
    });

    it('returns error for unknown client', () => {
      const result = manager.unsubscribe('ghost', [1]);
      expect(result.ok).toBe(false);
    });
  });

  // ── Event routing ───────────────────────────────────────────────────────

  describe('matchingClients', () => {
    it('returns empty set when no clients are subscribed', () => {
      const event = makeEvent({ match_id: 5 });
      expect(manager.matchingClients(event).size).toBe(0);
    });

    it('matches client subscribed by match ID', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [5]);

      const event = makeEvent({ match_id: 5 });
      const clients = manager.matchingClients(event);
      expect(clients.has(state.id)).toBe(true);
      expect(clients.size).toBe(1);
    });

    it('does NOT match client subscribed to different match ID', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [5]);

      const event = makeEvent({ match_id: 99 });
      expect(manager.matchingClients(event).size).toBe(0);
    });

    it('matches client subscribed to player1 address', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [], ['GABC']);

      const event = makeEvent({ match_id: 1, player1: 'GABC', player2: 'GXYZ' });
      const clients = manager.matchingClients(event);
      expect(clients.has(state.id)).toBe(true);
    });

    it('matches client subscribed to player2 address', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [], ['GDEF']);

      const event = makeEvent({ match_id: 1, player1: 'GABC', player2: 'GDEF' });
      expect(manager.matchingClients(event).has(state.id)).toBe(true);
    });

    it('matches on player address case-insensitively', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [], ['GABC']); // subscribed uppercase

      // event has lowercase player1
      const event = makeEvent({ match_id: 1, player1: 'gabc', player2: 'GDEF' });
      expect(manager.matchingClients(event).has(state.id)).toBe(true);
    });

    it('deduplicates clients matching by both match ID and player address', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [1], ['GABC']); // subscribed to both

      const event = makeEvent({ match_id: 1, player1: 'GABC' });
      const clients = manager.matchingClients(event);
      // Should appear exactly once
      expect(clients.size).toBe(1);
    });

    it('routes event to multiple different subscribers', () => {
      const s1 = makeState();
      const s2 = makeState();
      const s3 = makeState();
      manager.addClient(s1);
      manager.addClient(s2);
      manager.addClient(s3);

      manager.subscribe(s1.id, [10]);
      manager.subscribe(s2.id, [], ['GABC']);
      // s3 subscribed to a different match
      manager.subscribe(s3.id, [99]);

      const event = makeEvent({ match_id: 10, player1: 'GABC', player2: 'GDEF' });
      const clients = manager.matchingClients(event);
      expect(clients.has(s1.id)).toBe(true);
      expect(clients.has(s2.id)).toBe(true);
      expect(clients.has(s3.id)).toBe(false);
    });

    it('does not route to removed client', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [1]);
      manager.removeClient(state.id);

      const event = makeEvent({ match_id: 1 });
      expect(manager.matchingClients(event).size).toBe(0);
    });

    it('does not route to client that unsubscribed', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [1]);
      manager.unsubscribe(state.id, [1]);

      const event = makeEvent({ match_id: 1 });
      expect(manager.matchingClients(event).size).toBe(0);
    });

    it('handles events without player fields gracefully', () => {
      const state = makeState();
      manager.addClient(state);
      manager.subscribe(state.id, [1]);

      const event = makeEvent({ match_id: 1, player1: undefined, player2: undefined });
      const clients = manager.matchingClients(event);
      expect(clients.has(state.id)).toBe(true); // matched by match_id
    });
  });

  // ── Rate limiting ───────────────────────────────────────────────────────

  describe('rate limiting', () => {
    it('blocks subscribe after exceeding limit', () => {
      const mgr = new SubscriptionManager({
        ...DEFAULT_CONFIG,
        rateLimitMaxSubscribes: 3,
        rateLimitWindowMs: 60_000,
      });
      const state = makeState();
      mgr.addClient(state);

      // 3 commands should succeed
      mgr.subscribe(state.id, [1]);
      mgr.subscribe(state.id, [2]);
      mgr.subscribe(state.id, [3]);

      // 4th should be rate-limited
      const result = mgr.subscribe(state.id, [4]);
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/rate_limited/);
    });

    it('resets rate-limit counter after window expires', () => {
      const mgr = new SubscriptionManager({
        ...DEFAULT_CONFIG,
        rateLimitMaxSubscribes: 1,
        rateLimitWindowMs: 100,
      });
      const state = makeState({ rateLimitWindowStart: Date.now() - 200 }); // window already expired
      mgr.addClient(state);

      const result = mgr.subscribe(state.id, [1]);
      expect(result.ok).toBe(true);
    });
  });

  // ── Edge cases ──────────────────────────────────────────────────────────

  describe('edge cases', () => {
    it('handles subscribing to 0 match IDs and 0 addresses', () => {
      const state = makeState();
      manager.addClient(state);
      const result = manager.subscribe(state.id, [], []);
      expect(result.ok).toBe(true);
    });

    it('handles many clients subscribing to the same match', () => {
      const count = 500;
      for (let i = 0; i < count; i++) {
        const s = makeState();
        manager.addClient(s);
        manager.subscribe(s.id, [77]);
      }
      const event = makeEvent({ match_id: 77 });
      expect(manager.matchingClients(event).size).toBe(count);
    });

    it('allClients() returns all registered clients', () => {
      const s1 = makeState();
      const s2 = makeState();
      manager.addClient(s1);
      manager.addClient(s2);
      const all = Array.from(manager.allClients());
      expect(all).toHaveLength(2);
    });
  });
});
