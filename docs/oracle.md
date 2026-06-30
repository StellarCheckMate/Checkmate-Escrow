## Health Check

The oracle exposes a /health endpoint to monitor connectivity and uptime.

**Endpoint:** GET /health

**Response (200 OK):**
```json
{
  "status": "healthy",
  "network": "testnet",
  "contract_address": "CB...",
  "last_checked_at": "2026-06-30T18:00:00Z"
}
```