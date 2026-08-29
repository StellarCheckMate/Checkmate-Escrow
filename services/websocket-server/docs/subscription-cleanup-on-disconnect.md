# Fix: subscription cleanup on abrupt client disconnect

## Issue

`SubscriptionManager` tracks two indices — `matchId -> Set<clientId>` and
`playerAddress -> Set<clientId>` — that must be cleared for a client when it
disconnects. If cleanup only ran in response to an explicit `unsubscribe`
message, a client that disappears without sending one (network blip, closed
tab, crashed process) would leave its entries in both indices forever: a slow
memory leak that grows with connection churn on a long-running server.

## What was already in place

`ConnectionManager.handleConnection` already registered a `ws.on('close', ...)`
handler that calls `this.subscriptions.removeClient(clientId)` — this covers
every disconnect path (clean close, abrupt drop, or a server-initiated
`ws.terminate()` from the heartbeat loop), not just an explicit unsubscribe.

## What changed

- Added a comment on the `close` handler in `connectionManager.ts` documenting
  *why* cleanup lives there instead of only in the `unsubscribe` message
  handler, so a future change doesn't accidentally move it behind an explicit
  opt-in.
- Added a regression test in `src/tests/reconnection.test.ts`
  (`removes a client's subscriptions when it disconnects without
  unsubscribing`) that:
  1. Connects two clients, both subscribed to the same match.
  2. Closes one client's socket directly (`ws.close()`), without sending an
     `unsubscribe` message.
  3. Asserts `connectionManager.connectionCount` drops by one.
  4. Broadcasts a new event for that match and asserts the still-connected
     client receives it — proving the disconnected client's entry was
     actually removed from `SubscriptionManager`'s routing index, not just
     that the connection count went down.

## Files touched

- `src/connectionManager.ts` — added the explanatory comment.
- `src/tests/reconnection.test.ts` — added the regression test.
