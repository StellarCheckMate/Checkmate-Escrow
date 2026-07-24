/**
 * ConnectionManager
 *
 * Owns the ws.WebSocketServer instance and all per-connection lifecycle:
 *   - sends the 'welcome' message on connect
 *   - dispatches inbound client messages to SubscriptionManager
 *   - runs the heartbeat loop (ping every N seconds, close stale connections)
 *   - logs disconnect reasons
 *   - calls SubscriptionManager.removeClient on close
 *   - exposes a `broadcast(event)` method for the poller to call
 */

import { WebSocketServer, WebSocket } from 'ws';
import { randomUUID } from 'crypto';
import type {
  ClientMessage,
  ClientState,
  IndexedEvent,
  ServerConfig,
  ServerMessage,
} from './types.js';
import { PROTOCOL_VERSION } from './types.js';
import { SubscriptionManager } from './subscriptionManager.js';
import { logger } from './logger.js';

export class ConnectionManager {
  private wss: WebSocketServer;
  private readonly subscriptions: SubscriptionManager;
  /** Map clientId → raw WebSocket for message delivery */
  private readonly sockets = new Map<string, WebSocket>();
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly config: ServerConfig) {
    this.subscriptions = new SubscriptionManager({
      maxSubscriptionsPerClient: config.maxSubscriptionsPerClient,
      rateLimitMaxSubscribes: config.rateLimitMaxSubscribes,
      rateLimitWindowMs: config.rateLimitWindowMs,
    });

    this.wss = new WebSocketServer({
      host: config.host,
      port: config.port,
      // Limit back-pressure: each socket gets its own send queue
      perMessageDeflate: false,
    });
  }

  // ─── Lifecycle ─────────────────────────────────────────────────────────

  start(): void {
    this.wss.on('connection', (ws, req) => this.handleConnection(ws, req));
    this.wss.on('error', (err) => {
      logger.error({ err }, 'WebSocketServer error');
    });

    this.heartbeatTimer = setInterval(
      () => this.runHeartbeat(),
      this.config.heartbeatIntervalMs,
    );

    logger.info(
      { host: this.config.host, port: this.config.port },
      'WebSocket server listening',
    );
  }

  stop(): Promise<void> {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    return new Promise((resolve, reject) => {
      this.wss.close((err) => (err ? reject(err) : resolve()));
    });
  }

  get connectionCount(): number {
    return this.subscriptions.clientCount;
  }

  /** Push an event to all clients that subscribed to it */
  broadcast(event: IndexedEvent): void {
    const targets = this.subscriptions.matchingClients(event);
    if (targets.size === 0) return;

    const msg = JSON.stringify({ type: 'event', event } satisfies ServerMessage);

    for (const clientId of targets) {
      const ws = this.sockets.get(clientId);
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(msg, (err) => {
          if (err) {
            logger.warn({ clientId, err }, 'Failed to send event to client');
          }
        });
      }
    }

    logger.debug(
      { matchId: event.match_id, eventType: event.event_type, recipientCount: targets.size },
      'Broadcast event',
    );
  }

  // ─── Connection handler ────────────────────────────────────────────────

  private handleConnection(ws: WebSocket, req: import('http').IncomingMessage): void {
    const clientId = randomUUID();
    const remoteAddress =
      (req.headers['x-forwarded-for'] as string | undefined)?.split(',')[0]?.trim() ??
      req.socket.remoteAddress ??
      'unknown';

    const state: ClientState = {
      id: clientId,
      matchIds: new Set(),
      playerAddresses: new Set(),
      lastSeenAt: Date.now(),
      subscribeCount: 0,
      rateLimitWindowStart: Date.now(),
      remoteAddress,
    };

    this.subscriptions.addClient(state);
    this.sockets.set(clientId, ws);

    logger.info({ clientId, remoteAddress, total: this.connectionCount }, 'Client connected');

    // Send welcome
    this.send(ws, {
      type: 'welcome',
      protocol_version: PROTOCOL_VERSION,
      server_time: new Date().toISOString(),
    });

    ws.on('message', (data) => {
      state.lastSeenAt = Date.now();
      this.handleMessage(clientId, ws, data.toString());
    });

    ws.on('close', (code, reason) => {
      const reasonStr = reason.toString() || `code=${code}`;
      state.disconnectReason = reasonStr;
      logger.info(
        { clientId, remoteAddress, reason: reasonStr, total: this.connectionCount - 1 },
        'Client disconnected',
      );
      this.subscriptions.removeClient(clientId);
      this.sockets.delete(clientId);
    });

    ws.on('error', (err) => {
      state.disconnectReason = err.message;
      logger.warn({ clientId, remoteAddress, err }, 'Client socket error');
    });

    // Respond to built-in WebSocket pings
    ws.on('ping', () => {
      state.lastSeenAt = Date.now();
    });
  }

  // ─── Message dispatcher ────────────────────────────────────────────────

  private handleMessage(clientId: string, ws: WebSocket, raw: string): void {
    let msg: ClientMessage;
    try {
      msg = JSON.parse(raw) as ClientMessage;
    } catch {
      this.send(ws, {
        type: 'error',
        code: 'PARSE_ERROR',
        message: 'Message must be valid JSON',
      });
      return;
    }

    if (!msg || typeof msg.type !== 'string') {
      this.send(ws, {
        type: 'error',
        code: 'INVALID_MESSAGE',
        message: 'Missing required field: type',
      });
      return;
    }

    switch (msg.type) {
      case 'subscribe': {
        const result = this.subscriptions.subscribe(
          clientId,
          msg.payload?.match_ids,
          msg.payload?.player_addresses,
        );
        if (!result.ok) {
          const isRateLimit = result.error?.startsWith('rate_limited');
          if (isRateLimit) {
            this.send(ws, {
              type: 'rate_limited',
              message: result.error!,
              retry_after_ms: this.config.rateLimitWindowMs,
            });
          } else {
            this.send(ws, {
              type: 'error',
              code: 'SUBSCRIBE_ERROR',
              message: result.error!,
            });
          }
        } else {
          const clientState = this.subscriptions.getClient(clientId)!;
          this.send(ws, {
            type: 'subscribed',
            match_ids: Array.from(clientState.matchIds),
            player_addresses: Array.from(clientState.playerAddresses),
          });
        }
        break;
      }

      case 'unsubscribe': {
        const result = this.subscriptions.unsubscribe(
          clientId,
          msg.payload?.match_ids,
          msg.payload?.player_addresses,
        );
        if (!result.ok) {
          this.send(ws, {
            type: 'error',
            code: 'UNSUBSCRIBE_ERROR',
            message: result.error!,
          });
        } else {
          const clientState = this.subscriptions.getClient(clientId)!;
          this.send(ws, {
            type: 'unsubscribed',
            match_ids: msg.payload?.match_ids ?? [],
            player_addresses: msg.payload?.player_addresses ?? [],
          });
          logger.debug(
            {
              clientId,
              remaining: clientState.matchIds.size + clientState.playerAddresses.size,
            },
            'Client unsubscribed',
          );
        }
        break;
      }

      case 'ping':
        this.send(ws, { type: 'pong', server_time: new Date().toISOString() });
        break;

      default:
        this.send(ws, {
          type: 'error',
          code: 'UNKNOWN_MESSAGE_TYPE',
          message: `Unknown message type: ${(msg as { type: string }).type}`,
        });
    }
  }

  // ─── Heartbeat ─────────────────────────────────────────────────────────

  private runHeartbeat(): void {
    const now = Date.now();
    const timeout = this.config.heartbeatTimeoutMs;
    let pinged = 0;
    let terminated = 0;

    for (const state of this.subscriptions.allClients()) {
      const ws = this.sockets.get(state.id);
      if (!ws || ws.readyState !== WebSocket.OPEN) continue;

      if (now - state.lastSeenAt > timeout) {
        state.disconnectReason = 'heartbeat_timeout';
        ws.terminate();
        terminated += 1;
      } else {
        // Native WebSocket ping (distinct from app-level ping)
        ws.ping(undefined, false, (err) => {
          if (err) logger.warn({ clientId: state.id, err }, 'Heartbeat ping failed');
        });
        pinged += 1;
      }
    }

    if (pinged > 0 || terminated > 0) {
      logger.debug({ pinged, terminated }, 'Heartbeat cycle');
    }
  }

  // ─── Helpers ───────────────────────────────────────────────────────────

  private send(ws: WebSocket, msg: ServerMessage): void {
    if (ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(msg), (err) => {
      if (err) logger.warn({ err }, 'Failed to send message');
    });
  }
}
