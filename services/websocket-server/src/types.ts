/**
 * Checkmate-Escrow WebSocket Server — Shared Types
 *
 * Protocol version: 1
 *
 * All messages are JSON-encoded text frames.
 */

// ─── Protocol version ──────────────────────────────────────────────────────

export const PROTOCOL_VERSION = 1;

// ─── Subscription types ────────────────────────────────────────────────────

/** Subscribe by match ID, player address, or both */
export interface SubscribePayload {
  match_ids?: number[];
  player_addresses?: string[];
}

/** Unsubscribe from specific match IDs or player addresses */
export interface UnsubscribePayload {
  match_ids?: number[];
  player_addresses?: string[];
}

// ─── Client → Server messages ──────────────────────────────────────────────

export type ClientMessageType = 'subscribe' | 'unsubscribe' | 'ping';

export interface ClientSubscribeMessage {
  type: 'subscribe';
  payload: SubscribePayload;
}

export interface ClientUnsubscribeMessage {
  type: 'unsubscribe';
  payload: UnsubscribePayload;
}

export interface ClientPingMessage {
  type: 'ping';
}

export type ClientMessage =
  | ClientSubscribeMessage
  | ClientUnsubscribeMessage
  | ClientPingMessage;

// ─── Server → Client messages ──────────────────────────────────────────────

export type ServerMessageType =
  | 'welcome'
  | 'subscribed'
  | 'unsubscribed'
  | 'event'
  | 'pong'
  | 'error'
  | 'rate_limited';

export interface WelcomeMessage {
  type: 'welcome';
  protocol_version: number;
  server_time: string;
}

export interface SubscribedMessage {
  type: 'subscribed';
  match_ids: number[];
  player_addresses: string[];
}

export interface UnsubscribedMessage {
  type: 'unsubscribed';
  match_ids: number[];
  player_addresses: string[];
}

export interface MatchEventMessage {
  type: 'event';
  event: IndexedEvent;
}

export interface PongMessage {
  type: 'pong';
  server_time: string;
}

export interface ErrorMessage {
  type: 'error';
  code: string;
  message: string;
}

export interface RateLimitedMessage {
  type: 'rate_limited';
  message: string;
  retry_after_ms: number;
}

export type ServerMessage =
  | WelcomeMessage
  | SubscribedMessage
  | UnsubscribedMessage
  | MatchEventMessage
  | PongMessage
  | ErrorMessage
  | RateLimitedMessage;

// ─── Event model (mirrors event-indexer Rust model) ────────────────────────

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

// ─── Internal client state ─────────────────────────────────────────────────

export interface ClientState {
  /** Unique connection ID (UUID v4) */
  id: string;
  /** Match IDs this client is watching */
  matchIds: Set<number>;
  /** Player addresses this client is watching */
  playerAddresses: Set<string>;
  /** Epoch ms of last message received (for heartbeat) */
  lastSeenAt: number;
  /** Number of subscribe/unsubscribe commands in the current rate-limit window */
  subscribeCount: number;
  /** Epoch ms when the current rate-limit window started */
  rateLimitWindowStart: number;
  /** Remote address for logging */
  remoteAddress: string;
  /** Disconnect reason – populated on close for logging */
  disconnectReason?: string;
}

// ─── Config ────────────────────────────────────────────────────────────────

export interface ServerConfig {
  /** TCP port to listen on. Default: 8090 */
  port: number;
  /** Bind address. Default: 0.0.0.0 */
  host: string;
  /** Base URL of the event-indexer REST API. Default: http://localhost:8080 */
  eventIndexerUrl: string;
  /** How often (ms) to poll event-indexer for new events. Default: 5000 */
  pollIntervalMs: number;
  /** Interval (ms) to send server-initiated pings. Default: 30000 */
  heartbeatIntervalMs: number;
  /** Time (ms) before a non-responsive connection is terminated. Default: 60000 */
  heartbeatTimeoutMs: number;
  /** Max subscribe/unsubscribe commands per rate-limit window per client. Default: 20 */
  rateLimitMaxSubscribes: number;
  /** Rate-limit window duration (ms). Default: 60000 */
  rateLimitWindowMs: number;
  /** Max total subscriptions (matchIds + playerAddresses) a single client may hold. Default: 50 */
  maxSubscriptionsPerClient: number;
  /** Log level. Default: info */
  logLevel: string;
}

export function loadConfig(): ServerConfig {
  return {
    port: Number(process.env.WS_PORT ?? 8090),
    host: process.env.WS_HOST ?? '0.0.0.0',
    eventIndexerUrl:
      process.env.EVENT_INDEXER_URL ?? 'http://localhost:8080',
    pollIntervalMs: Number(process.env.WS_POLL_INTERVAL_MS ?? 5000),
    heartbeatIntervalMs: Number(process.env.WS_HEARTBEAT_INTERVAL_MS ?? 30_000),
    heartbeatTimeoutMs: Number(process.env.WS_HEARTBEAT_TIMEOUT_MS ?? 60_000),
    rateLimitMaxSubscribes: Number(process.env.WS_RATE_LIMIT_MAX_SUBSCRIBES ?? 20),
    rateLimitWindowMs: Number(process.env.WS_RATE_LIMIT_WINDOW_MS ?? 60_000),
    maxSubscriptionsPerClient: Number(process.env.WS_MAX_SUBSCRIPTIONS_PER_CLIENT ?? 50),
    logLevel: process.env.LOG_LEVEL ?? 'info',
  };
}
