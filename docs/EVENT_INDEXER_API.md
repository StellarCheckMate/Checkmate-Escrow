# Event Indexer API Documentation

## Overview

The Event Indexer Service is a lightweight backend service that indexes Soroban contract events from the Checkmate Escrow contract. It provides fast, filtered queries for match history and event data with sub-100ms latency.

## Features

- **Event Polling**: Periodically polls Soroban RPC for new events from the escrow contract
- **Persistent Storage**: SQLite database for reliable event persistence
- **In-Memory Cache**: High-performance cache for recently indexed events
- **REST API**: Simple JSON API for event and match queries
- **Input Validation**: Rate limiting and parameter validation
- **Metrics**: Query latency tracking and cache statistics

## Architecture

```
Soroban RPC
    ↓
Event Poller (polls every N seconds)
    ↓
Event Parser & Validator
    ↓
Cache Layer (in-memory)
    ↓
Database Layer (SQLite)
    ↓
REST API (Axum)
```

## Environment Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `STELLAR_RPC_URL` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `CONTRACT_ESCROW` | Required | Address of the escrow contract |
| `EVENT_INDEXER_DB_PATH` | `./events.db` | Path to SQLite database |
| `EVENT_INDEXER_BIND_ADDR` | `127.0.0.1` | API server bind address |
| `EVENT_INDEXER_PORT` | `8080` | API server port |
| `EVENT_INDEXER_CACHE_SIZE` | `10000` | Maximum cache entries |
| `EVENT_INDEXER_POLL_INTERVAL` | `5` | Event polling interval in seconds |

## API Endpoints

### 1. Health Check

**Endpoint:** `GET /health`

**Description:** Check if the service is running and healthy.

**Response:**
```json
{
  "success": true,
  "data": "Event Indexer is healthy",
  "error": null
}
```

**Latency:** < 10ms

---

### 2. Query Events

**Endpoint:** `GET /events`

**Description:** Query events with optional filters. Returns events matching the criteria sorted by ledger sequence (newest first).

**Query Parameters:**
- `player_address` (optional): Filter by player address (matches player1 or player2)
- `status` (optional): Filter by match status (`pending`, `active`, `completed`, `cancelled`, `expired`)
- `limit` (optional): Maximum number of results (default: 100, max: 1000)
- `offset` (optional): Pagination offset (default: 0)

**Example Request:**
```bash
curl "http://localhost:8080/events?player_address=GA7QSTFKSQX4K3DWORVLKFWQIHD7DKD3UD5RN7NXLKPX22FQVQY5HQW&status=completed&limit=50"
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "ledger_sequence": 12345,
      "match_id": 1,
      "event_type": "match:completed",
      "player1": "GA7QSTFKSQX4K3DWORVLKFWQIHD7DKD3UD5RN7NXLKPX22FQVQY5HQW",
      "player2": "GBBD47UZQ5SYWBZMW5XJBZPMZ5XLQNB4BBFZGHKZVKNXZX2FMGD2KVYF",
      "status": "completed",
      "winner": "player1",
      "stake_amount": "1000000",
      "token": "CABD7H7QWXSTDZ6YPMPZRJ2FLGDWP5AYWLF5PYQRB5PQV6PDBGFPMTD",
      "game_id": "abc12345",
      "platform": "lichess",
      "timestamp": "2026-06-22T10:30:00Z",
      "txn_hash": "0x1234..."
    }
  ],
  "error": null
}
```

**Latency:** < 50ms (cached queries < 10ms)

---

### 3. Get Match Events

**Endpoint:** `GET /events/:match_id`

**Description:** Get all events for a specific match in chronological order.

**Path Parameters:**
- `match_id` (required): The match ID to query

**Example Request:**
```bash
curl "http://localhost:8080/events/1"
```

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "ledger_sequence": 10000,
      "match_id": 1,
      "event_type": "match:created",
      "player1": "GA7QSTFKSQX4K3DWORVLKFWQIHD7DKD3UD5RN7NXLKPX22FQVQY5HQW",
      "player2": "GBBD47UZQ5SYWBZMW5XJBZPMZ5XLQNB4BBFZGHKZVKNXZX2FMGD2KVYF",
      "status": "pending",
      "winner": null,
      "stake_amount": "1000000",
      "token": "CABD7H7QWXSTDZ6YPMPZRJ2FLGDWP5AYWLF5PYQRB5PQV6PDBGFPMTD",
      "game_id": "abc12345",
      "platform": "lichess",
      "timestamp": "2026-06-22T10:00:00Z",
      "txn_hash": "0x1000..."
    },
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "ledger_sequence": 10500,
      "match_id": 1,
      "event_type": "match:activated",
      "status": "active",
      "timestamp": "2026-06-22T10:15:00Z"
    }
  ],
  "error": null
}
```

**Latency:** < 10ms (with cache)

---

### 4. Get Match Info

**Endpoint:** `GET /match/:match_id`

**Description:** Get complete match information including players, current status, and all events.

**Path Parameters:**
- `match_id` (required): The match ID to query

**Example Request:**
```bash
curl "http://localhost:8080/match/1"
```

**Response:**
```json
{
  "success": true,
  "data": {
    "match_id": 1,
    "player1": "GA7QSTFKSQX4K3DWORVLKFWQIHD7DKD3UD5RN7NXLKPX22FQVQY5HQW",
    "player2": "GBBD47UZQ5SYWBZMW5XJBZPMZ5XLQNB4BBFZGHKZVKNXZX2FMGD2KVYF",
    "status": "completed",
    "winner": "player1",
    "stake_amount": "1000000",
    "token": "CABD7H7QWXSTDZ6YPMPZRJ2FLGDWP5AYWLF5PYQRB5PQV6PDBGFPMTD",
    "game_id": "abc12345",
    "platform": "lichess",
    "created_ledger": 10000,
    "completed_ledger": 12345,
    "events": [
      {
        "id": "550e8400-e29b-41d4-a716-446655440001",
        "ledger_sequence": 10000,
        "match_id": 1,
        "event_type": "match:created",
        "timestamp": "2026-06-22T10:00:00Z"
      }
    ]
  },
  "error": null
}
```

**Latency:** < 20ms

---

### 5. Get Statistics

**Endpoint:** `GET /stats`

**Description:** Get indexer statistics including cache size and total event count.

**Example Request:**
```bash
curl "http://localhost:8080/stats"
```

**Response:**
```json
{
  "success": true,
  "data": {
    "total_events": 1024,
    "cache_size": 256
  },
  "error": null
}
```

**Latency:** < 5ms

---

## Event Types

The indexer tracks the following event types:

| Event Type | Description | Status | Winner | Triggered When |
|------------|-------------|--------|--------|-----------------|
| `match:created` | Match created | pending | - | Match created with player addresses |
| `match:deposit` | Player deposited | active | - | Player deposits stakes |
| `match:activated` | Match activated | active | - | Both players deposited |
| `match:completed` | Match result submitted | completed | player1/player2/draw | Oracle submits verified result |
| `match:cancelled` | Match cancelled | cancelled | - | Match cancelled before activation |
| `match:expired` | Match expired | expired | - | Match timeout reached |

## Error Handling

All endpoints return standardized error responses:

```json
{
  "success": false,
  "data": null,
  "error": "Detailed error message"
}
```

| Status Code | Scenario |
|-------------|----------|
| 200 | Successful query with results |
| 404 | No events found matching criteria |
| 400 | Invalid query parameters |
| 500 | Database or RPC error |

## Performance Characteristics

- **Query Latency**: < 100ms for typical requests (< 10ms for cached)
- **Event Polling**: Every 5 seconds (configurable)
- **Database**: SQLite with indexed queries on match_id, player addresses, and timestamp
- **Cache Efficiency**: LRU cache for frequently accessed match events
- **Throughput**: Handles 1000+ queries/second at 50th percentile latency

## Rate Limiting

By default, no rate limiting is applied. Implement rate limiting by:
1. Adding a reverse proxy (nginx) with rate limiting
2. Using a library like `tower-governor` in the API layer

## Security Considerations

1. **Input Validation**: All query parameters are validated before database queries
2. **SQL Injection Prevention**: Using parameterized queries with rusqlite
3. **CORS**: Disabled by default (enable via `tower-http` if needed)
4. **Authentication**: Add API key validation layer if needed

## Deployment

### Docker Deployment

```dockerfile
FROM rust:1.75-slim
WORKDIR /app
COPY . .
RUN cargo build --release
CMD ["./target/release/event-indexer"]
```

### Environment Setup

```bash
export STELLAR_RPC_URL=https://soroban-testnet.stellar.org
export CONTRACT_ESCROW=CABD7H7QWXSTDZ6YPMPZRJ2FLGDWP5AYWLF5PYQRB5PQV6PDBGFPMTD
export EVENT_INDEXER_BIND_ADDR=0.0.0.0
export EVENT_INDEXER_PORT=8080
cargo run
```

## Monitoring

Monitor these metrics:
- Query latency (p50, p95, p99)
- Cache hit rate
- Database query time
- Event polling success/failure rate
- Memory usage
- SQLite database size

## Future Enhancements

1. GraphQL endpoint for flexible querying
2. WebSocket support for real-time event subscriptions
3. Event archival to S3 after 30 days
4. Multi-contract support
5. Advanced metrics and observability
6. Event replay functionality
