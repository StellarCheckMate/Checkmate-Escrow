/**
 * Checkmate-Escrow WebSocket Server — Entry Point
 *
 * Reads config from environment, starts the WebSocket server, and kicks off
 * the event poller which feeds broadcasted events to subscribed clients.
 */

import { loadConfig } from './types.js';
import { ConnectionManager } from './connectionManager.js';
import { EventPoller } from './eventPoller.js';
import { logger } from './logger.js';

const config = loadConfig();

const connectionManager = new ConnectionManager(config);
connectionManager.start();

const poller = new EventPoller(config, (event) => {
  connectionManager.broadcast(event);
});
poller.start();

logger.info(
  {
    wsPort: config.port,
    eventIndexerUrl: config.eventIndexerUrl,
    pollIntervalMs: config.pollIntervalMs,
    heartbeatIntervalMs: config.heartbeatIntervalMs,
  },
  'Checkmate WebSocket server started',
);

// ─── Graceful shutdown ────────────────────────────────────────────────────

async function shutdown(signal: string): Promise<void> {
  logger.info({ signal }, 'Shutting down');
  poller.stop();
  await connectionManager.stop();
  logger.info('Shutdown complete');
  process.exit(0);
}

process.on('SIGTERM', () => void shutdown('SIGTERM'));
process.on('SIGINT', () => void shutdown('SIGINT'));
