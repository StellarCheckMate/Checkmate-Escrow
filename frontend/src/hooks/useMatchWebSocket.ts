/**
 * useMatchWebSocket
 *
 * React hook that maintains a WebSocket connection to the Checkmate-Escrow
 * real-time server and delivers match events to the caller.
 *
 * Features:
 *  - Automatic connection with 'welcome' handshake
 *  - Subscribes to match IDs and/or player addresses
 *  - Exponential back-off reconnect (1 s → 2 s → … capped at 30 s)
 *  - Responds to server-initiated pings with pong
 *  - Re-subscribes automatically after reconnect
 *  - Returns connection status, latest event, and full event history
 *  - Clean-up on unmount
 *
 * Protocol version: 1
 */

import { useCallback, useEffect, useRef, useState } from 'react';

// ─── Types (subset of the server's type definitions) ─────────────────────────

export interface IndexedEvent {
  id: string;
  ledger_sequence: number;
  match_id: number;
  event_type: string;
  player1?: string;
  player2?: string;
  status?: string;
  winner?: string;
  stake_amount?: string;
  token?: string;
  game_id?: string;
  platform?: string;
  timestamp: string;
  txn_hash?: string;
  event_index_in_txn?: number;
}

export type ConnectionStatus =
  | 'connecting'
  | 'connected'
  | 'subscribing'
  | 'ready'
  | 'reconnecting'
  | 'error'
  | 'closed';

export interface UseMatchWebSocketOptions {
  /** Match IDs to subscribe to */
  matchIds?: number[];
  /** Player Stellar addresses to subscribe to */
  playerAddresses?: string[];
  /**
   * WebSocket server URL.
   * Defaults to VITE_WS_SERVER_URL env var, falling back to ws://localhost:8090
   */
  url?: string;
  /**
   * Whether the hook should be active.
   * Set to false to disable the connection without unmounting the component.
   * @default true
   */
  enabled?: boolean;
  /**
   * Maximum history length (number of events to retain).
   * @default 100
   */
  maxHistory?: number;
  /**
   * Called for every event that arrives (regardless of history).
   */
  onEvent?: (event: IndexedEvent) => void;
}

export interface UseMatchWebSocketReturn {
  status: ConnectionStatus;
  /** The most recently received event, or null if none yet */
  latestEvent: IndexedEvent | null;
  /** All events received since mount (capped at maxHistory) */
  events: IndexedEvent[];
  /** Error message if status === 'error' */
  error: string | null;
  /** Force a re-connection (e.g. after an error) */
  reconnect: () => void;
}

// ─── Internal message shapes ──────────────────────────────────────────────────

interface WelcomeMsg { type: 'welcome'; protocol_version: number; server_time: string }
interface SubscribedMsg { type: 'subscribed'; match_ids: number[]; player_addresses: string[] }
interface EventMsg { type: 'event'; event: IndexedEvent }
interface PingMsg { type: 'ping' }
interface RateLimitedMsg { type: 'rate_limited'; message: string; retry_after_ms: number }
interface ErrorMsg { type: 'error'; code: string; message: string }
type ServerMsg = WelcomeMsg | SubscribedMsg | EventMsg | PingMsg | RateLimitedMsg | ErrorMsg | { type: string };

// ─── Constants ────────────────────────────────────────────────────────────────

const MAX_BACKOFF_MS = 30_000;
const BASE_BACKOFF_MS = 1_000;

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * Connects to the Checkmate-Escrow WebSocket server and subscribes to
 * real-time match events for the given match IDs and/or player addresses.
 *
 * @example
 * ```tsx
 * const { status, latestEvent, events } = useMatchWebSocket({
 *   matchIds: [42],
 *   playerAddresses: ['GABC...'],
 *   onEvent: (e) => console.log('new event', e),
 * });
 * ```
 */
export function useMatchWebSocket({
  matchIds = [],
  playerAddresses = [],
  url,
  enabled = true,
  maxHistory = 100,
  onEvent,
}: UseMatchWebSocketOptions = {}): UseMatchWebSocketReturn {
  const resolvedUrl =
    url ??
    (typeof import.meta !== 'undefined' && (import.meta as { env?: Record<string, string> }).env?.VITE_WS_SERVER_URL) ??
    'ws://localhost:8090';

  const [status, setStatus] = useState<ConnectionStatus>('connecting');
  const [latestEvent, setLatestEvent] = useState<IndexedEvent | null>(null);
  const [events, setEvents] = useState<IndexedEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  const wsRef = useRef<WebSocket | null>(null);
  const retryCountRef = useRef(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Stable refs to options so reconnect callback can read latest values */
  const matchIdsRef = useRef(matchIds);
  const playerAddressesRef = useRef(playerAddresses);
  const onEventRef = useRef(onEvent);
  const enabledRef = useRef(enabled);
  const reconnectRequestedRef = useRef(false);

  // Keep refs in sync with props without triggering reconnect
  useEffect(() => { matchIdsRef.current = matchIds; }, [matchIds]);
  useEffect(() => { playerAddressesRef.current = playerAddresses; }, [playerAddresses]);
  useEffect(() => { onEventRef.current = onEvent; }, [onEvent]);
  useEffect(() => { enabledRef.current = enabled; }, [enabled]);

  const clearRetryTimer = useCallback(() => {
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, []);

  const connect = useCallback(() => {
    if (!enabledRef.current) return;

    // Close any existing socket
    if (wsRef.current) {
      wsRef.current.onclose = null; // prevent triggering reconnect loop
      wsRef.current.close();
      wsRef.current = null;
    }

    setStatus('connecting');
    setError(null);

    let ws: WebSocket;
    try {
      ws = new WebSocket(resolvedUrl);
    } catch (err) {
      setStatus('error');
      setError(String(err));
      return;
    }

    wsRef.current = ws;

    ws.onopen = () => {
      // Wait for 'welcome' before subscribing
    };

    ws.onmessage = (evt) => {
      let msg: ServerMsg;
      try {
        msg = JSON.parse(evt.data as string) as ServerMsg;
      } catch {
        return;
      }

      switch (msg.type) {
        case 'welcome': {
          retryCountRef.current = 0; // reset back-off on successful handshake
          setStatus('subscribing');
          const ids = matchIdsRef.current;
          const addrs = playerAddressesRef.current;
          if (ids.length > 0 || addrs.length > 0) {
            ws.send(
              JSON.stringify({
                type: 'subscribe',
                payload: { match_ids: ids, player_addresses: addrs },
              }),
            );
          } else {
            setStatus('ready');
          }
          break;
        }

        case 'subscribed':
          setStatus('ready');
          break;

        case 'event': {
          const event = (msg as EventMsg).event;
          setLatestEvent(event);
          setEvents((prev) => {
            const next = [...prev, event];
            return next.length > maxHistory ? next.slice(next.length - maxHistory) : next;
          });
          onEventRef.current?.(event);
          break;
        }

        case 'ping':
          ws.send(JSON.stringify({ type: 'pong', server_time: new Date().toISOString() }));
          break;

        case 'rate_limited':
          setError((msg as RateLimitedMsg).message);
          break;

        case 'error':
          setError(`${(msg as ErrorMsg).code}: ${(msg as ErrorMsg).message}`);
          break;
      }
    };

    ws.onclose = (evt) => {
      wsRef.current = null;
      if (!enabledRef.current) {
        setStatus('closed');
        return;
      }

      const reason = evt.reason || `code ${evt.code}`;
      const backoff = Math.min(BASE_BACKOFF_MS * 2 ** retryCountRef.current, MAX_BACKOFF_MS);
      const jitter = Math.random() * 500;
      retryCountRef.current += 1;

      setStatus('reconnecting');
      setError(`Disconnected (${reason}). Reconnecting in ${Math.round(backoff)}ms…`);

      retryTimerRef.current = setTimeout(() => {
        if (enabledRef.current || reconnectRequestedRef.current) {
          reconnectRequestedRef.current = false;
          connect();
        }
      }, backoff + jitter);
    };

    ws.onerror = () => {
      // onerror is always followed by onclose; let onclose handle retry
      setError('WebSocket error');
    };
  }, [resolvedUrl, maxHistory]); // only changes when URL or history cap changes

  // Re-subscribe when matchIds / playerAddresses change after connection
  useEffect(() => {
    const ws = wsRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN || status !== 'ready') return;
    if (matchIds.length === 0 && playerAddresses.length === 0) return;

    ws.send(
      JSON.stringify({
        type: 'subscribe',
        payload: { match_ids: matchIds, player_addresses: playerAddresses },
      }),
    );
  }, [matchIds, playerAddresses, status]);

  // Connect on mount / when enabled changes
  useEffect(() => {
    if (!enabled) {
      clearRetryTimer();
      if (wsRef.current) {
        wsRef.current.onclose = null;
        wsRef.current.close();
        wsRef.current = null;
      }
      setStatus('closed');
      return;
    }

    connect();

    return () => {
      clearRetryTimer();
      if (wsRef.current) {
        wsRef.current.onclose = null;
        wsRef.current.close();
        wsRef.current = null;
      }
    };
    // connect is stable (memoised); enabled controls the gate
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  const reconnect = useCallback(() => {
    clearRetryTimer();
    reconnectRequestedRef.current = true;
    retryCountRef.current = 0;
    connect();
  }, [connect, clearRetryTimer]);

  return { status, latestEvent, events, error, reconnect };
}
