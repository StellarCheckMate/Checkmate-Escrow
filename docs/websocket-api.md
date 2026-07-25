# WebSocket API — Real-Time Match Updates

The Checkmate-Escrow WebSocket server (`services/websocket-server`) delivers
real-time match events over a persistent WebSocket connection.

**Protocol version:** `1`  
**Default port:** `8090`  
**Wire format:** JSON text frames  
**Endpoint:** `ws://<host>:<port>/`

---

## Contents

1. [Quick start](#1-quick-start)
2. [Connection lifecycle](#2-connection-lifecycle)
3. [Client → Server messages](#3-client--server-messages)
4. [Server → Client messages](#4-server--client-messages)
5. [Event types](#5-event-types)
6. [Subscription model](#6-subscription-model)
7. [Reconnect strategy](#7-reconnect-strategy)
8. [Rate limiting](#8-rate-limiting)
9. [Heartbeat](#9-heartbeat)
10. [Security considerations](#10-security-considerations)
11. [Configuration reference](#11-configuration-reference)
12. [React hook](#12-react-hook)
13. [Protocol changelog](#13-protocol-changelog)

---

## 1. Quick start

```js
const ws = new WebSocket('ws://localhost:8090');

ws.onmessage = ({ data }) => {
  const msg = JSON.parse(data);

  if (msg.type === 'welcome') {
    // Subscribe to match 42 and to a player address
    ws.send(JSON.stringify({
      type: 'subscribe',
      payload: {
        match_ids: [42],
        player_addresses: ['GABC...XYZ'],
      },
    }));
  }

  if (msg.type === 'event') {
    console.log('New match event:', msg.event);
  }
};
```

---

## 2. Connection lifecycle

```
Client                           Server
  |                                |
  |──── TCP / WS handshake ────────▶|
  |◀─── welcome ───────────────────|   (protocol_version, server_time)
  |                                |
  |──── subscribe ─────────────────▶|   (match_ids, player_addresses)
  |◀─── subscribed ────────────────|   (echo of active subscriptions)
  |                                |
  |◀─── event (push) ──────────────|   (whenever a new event is indexed)
  |◀─── event (push) ──────────────|
  |                                |
  |──── ping ──────────────────────▶|   (optional app-level ping)
  |◀─── pong ──────────────────────|
  |                                |
  |──── close ─────────────────────▶|
```

The server also sends native WebSocket **ping frames** on the heartbeat
interval.  Compliant clients (browsers, `ws` library) respond automatically.

---

## 3. Client → Server messages

All messages are JSON objects with a `type` field.

### `subscribe`

Subscribe to events for specific match IDs or player addresses.  
Can be sent multiple times; subscriptions are **additive**.

```json
{
  "type": "subscribe",
  "payload": {
    "match_ids": [42, 100],
    "player_addresses": ["GABC...XYZ", "GDEF...UVW"]
  }
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `payload.match_ids` | `number[]` | no | Soroban match IDs (non-negative integers) |
| `payload.player_addresses` | `string[]` | no | Stellar account IDs (case-insensitive) |

At least one of the two fields must be non-empty.

### `unsubscribe`

Remove specific match IDs or player addresses from active subscriptions.

```json
{
  "type": "unsubscribe",
  "payload": {
    "match_ids": [42],
    "player_addresses": []
  }
}
```

### `ping`

Application-level ping.  The server replies with `pong`.

```json
{ "type": "ping" }
```

---

## 4. Server → Client messages

### `welcome`

Sent immediately on connection.

```json
{
  "type": "welcome",
  "protocol_version": 1,
  "server_time": "2026-07-24T13:02:08.276Z"
}
```

**Clients must wait for `welcome` before sending any commands.**

### `subscribed`

Acknowledgement sent after a successful `subscribe` command.  Contains the
**full** current subscription state for the connection, not just the newly
added entries.

```json
{
  "type": "subscribed",
  "match_ids": [42, 100],
  "player_addresses": ["gabc...xyz"]
}
```

Note: player addresses are always stored and returned in lower-case.

### `unsubscribed`

Acknowledgement sent after a successful `unsubscribe` command.

```json
{
  "type": "unsubscribed",
  "match_ids": [42],
  "player_addresses": []
}
```

### `event`

Pushed to clients whose subscriptions match the event.

```json
{
  "type": "event",
  "event": {
    "id": "a1b2c3d4-...",
    "ledger_sequence": 123456,
    "match_id": 42,
    "event_type": "match/created",
    "player1": "GABC...XYZ",
    "player2": "GDEF...UVW",
    "status": "pending",
    "winner": null,
    "stake_amount": "10000000",
    "token": "USDC_TOKEN_ADDRESS",
    "game_id": "abcdef123456",
    "platform": "Lichess",
    "timestamp": "2026-07-24T13:02:08.276Z",
    "txn_hash": "abc123...",
    "event_index_in_txn": 0
  }
}
```

See [Event types](#5-event-types) for all possible `event_type` values.

### `pong`

Response to a client `ping`.

```json
{
  "type": "pong",
  "server_time": "2026-07-24T13:02:10.100Z"
}
```

### `error`

Sent when the server rejects a command.

```json
{
  "type": "error",
  "code": "PARSE_ERROR",
  "message": "Message must be valid JSON"
}
```

| Code | Cause |
|---|---|
| `PARSE_ERROR` | Frame body is not valid JSON |
| `INVALID_MESSAGE` | JSON is valid but `type` field is missing |
| `UNKNOWN_MESSAGE_TYPE` | `type` is not a recognised command |
| `SUBSCRIBE_ERROR` | Invalid payload or subscription cap exceeded |
| `UNSUBSCRIBE_ERROR` | Client not found or invalid payload |

### `rate_limited`

Sent when the client exceeds the subscribe/unsubscribe command rate.

```json
{
  "type": "rate_limited",
  "message": "rate_limited: max 20 subscription commands per 60000ms",
  "retry_after_ms": 60000
}
```

The connection is **not** closed.  The client should back off for at least
`retry_after_ms` milliseconds before retrying.

---

## 5. Event types

These mirror the Soroban contract events defined in the [README](../README.md#events-reference).

| `event_type` | Emitted when | Key payload fields |
|---|---|---|
| `match/created` | `create_match` called | `player1`, `player2`, `stake_amount`, `token`, `game_id`, `platform` |
| `match/completed` | `submit_result` called | `winner` (`player1` / `player2` / `draw`) |
| `match/cancelled` | `cancel_match` called | — |
| `match/expired` | `expire_match` called | — |

---

## 6. Subscription model

### How routing works

The server maintains two in-memory indices:

- **Match index:** `match_id → Set<clientId>`
- **Player index:** `player_address (lower-case) → Set<clientId>`

When an event arrives, the server looks up both indices (by `match_id` and by
`player1`/`player2`) and delivers the event to the **union** of matched client
IDs.  A client that matches on both channels receives the event exactly once.

### Limits

| Limit | Default | Env var |
|---|---|---|
| Subscriptions per client (matchIds + playerAddresses) | 50 | `WS_MAX_SUBSCRIPTIONS_PER_CLIENT` |
| Subscribe/unsubscribe commands per window | 20 | `WS_RATE_LIMIT_MAX_SUBSCRIBES` |
| Rate-limit window | 60 s | `WS_RATE_LIMIT_WINDOW_MS` |

---

## 7. Reconnect strategy

The server does not send a `reconnect` signal.  Clients should implement
automatic reconnect with **exponential back-off**:

```
delay = min(BASE_BACKOFF × 2^attempt, MAX_BACKOFF) + random_jitter(0–500ms)
```

| Parameter | Recommended value |
|---|---|
| `BASE_BACKOFF` | 1 000 ms |
| `MAX_BACKOFF` | 30 000 ms |
| Jitter | 0–500 ms |

After reconnect, clients must re-subscribe because all server-side state is
ephemeral.  The provided [React hook](#12-react-hook) handles this
automatically.

---

## 8. Rate limiting

Subscribe and unsubscribe commands count against a rolling per-client window.
Exceeding the limit triggers a `rate_limited` message.  The connection stays
open; wait `retry_after_ms` before sending more subscription commands.

---

## 9. Heartbeat

The server sends a native WebSocket **ping frame** every
`WS_HEARTBEAT_INTERVAL_MS` milliseconds (default 30 s).  Connections that have
not sent any frame for `WS_HEARTBEAT_TIMEOUT_MS` (default 60 s) are terminated
with reason `heartbeat_timeout`.

Disconnection reasons are written to the structured log at `info` level.

---

## 10. Security considerations

| Concern | Mitigation |
|---|---|
| Unauthorised data access | Clients only receive events for match IDs / player addresses they explicitly subscribed to.  The server never broadcasts all-events. |
| Subscription flooding | Per-client rate limit (`WS_RATE_LIMIT_MAX_SUBSCRIBES`) prevents abuse. |
| Slow-client back-pressure | Each socket has its own send queue; a slow consumer is terminated via heartbeat timeout rather than blocking other clients. |
| TLS | Deploy behind a TLS-terminating reverse proxy (nginx, Caddy, ALB) for production. |
| Authentication | The current implementation does not require authentication.  For production, add JWT verification in the `connection` handler before sending `welcome`. |

---

## 11. Configuration reference

All settings are read from environment variables at startup.

| Env var | Default | Description |
|---|---|---|
| `WS_PORT` | `8090` | TCP port to listen on |
| `WS_HOST` | `0.0.0.0` | Bind address |
| `EVENT_INDEXER_URL` | `http://localhost:8080` | Event-indexer REST API base URL |
| `WS_POLL_INTERVAL_MS` | `5000` | How often to poll event-indexer for new events (ms) |
| `WS_HEARTBEAT_INTERVAL_MS` | `30000` | Interval between server-initiated pings (ms) |
| `WS_HEARTBEAT_TIMEOUT_MS` | `60000` | Max silence before connection is terminated (ms) |
| `WS_RATE_LIMIT_MAX_SUBSCRIBES` | `20` | Max subscribe/unsubscribe commands per window |
| `WS_RATE_LIMIT_WINDOW_MS` | `60000` | Rate-limit window duration (ms) |
| `WS_MAX_SUBSCRIPTIONS_PER_CLIENT` | `50` | Max active subscriptions per connection |
| `LOG_LEVEL` | `info` | Pino log level (`trace`/`debug`/`info`/`warn`/`error`) |

---

## 12. React hook

The frontend ships a `useMatchWebSocket` hook that wraps the protocol:

```tsx
import { useMatchWebSocket } from './hooks/useMatchWebSocket';

function MatchCard({ matchId }: { matchId: number }) {
  const { status, latestEvent, events, error, reconnect } = useMatchWebSocket({
    matchIds: [matchId],
    // Optionally also track a player's address across all matches:
    // playerAddresses: [walletAddress],
    url: import.meta.env.VITE_WS_SERVER_URL, // defaults to ws://localhost:8090
    onEvent: (event) => {
      console.log('Real-time event:', event.event_type, event);
    },
  });

  if (status === 'error') return <button onClick={reconnect}>Reconnect</button>;
  if (latestEvent?.event_type === 'match/completed') {
    return <div>Winner: {latestEvent.winner}</div>;
  }
  return <div>Status: {status} — {events.length} events received</div>;
}
```

### Hook API

| Option | Type | Default | Description |
|---|---|---|---|
| `matchIds` | `number[]` | `[]` | Match IDs to subscribe to |
| `playerAddresses` | `string[]` | `[]` | Stellar addresses to subscribe to |
| `url` | `string` | `VITE_WS_SERVER_URL` or `ws://localhost:8090` | WebSocket server URL |
| `enabled` | `boolean` | `true` | Set `false` to disable without unmounting |
| `maxHistory` | `number` | `100` | Max events kept in `events` array |
| `onEvent` | `(event) => void` | — | Called for every incoming event |

| Return value | Type | Description |
|---|---|---|
| `status` | `ConnectionStatus` | `connecting` / `subscribing` / `ready` / `reconnecting` / `error` / `closed` |
| `latestEvent` | `IndexedEvent \| null` | Most recent event |
| `events` | `IndexedEvent[]` | History (capped at `maxHistory`) |
| `error` | `string \| null` | Last error message |
| `reconnect` | `() => void` | Force immediate reconnect |

---

## 13. Protocol changelog

| Version | Date | Changes |
|---|---|---|
| **1** | 2026-07-24 | Initial release — `subscribe`, `unsubscribe`, `ping/pong`, `welcome`, `subscribed`, `unsubscribed`, `event`, `error`, `rate_limited` messages |
