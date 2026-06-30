import { useState } from 'react';
import { useAdminContract } from '../hooks/useAdminContract';
import { ConfirmDialog } from '../components/admin/ConfirmDialog';
import type { WalletState, WalletType } from '../wallets/types';

interface Props {
  wallet: WalletState & { connect: (type: WalletType) => Promise<void>; disconnect: () => void };
}

type PendingAction =
  | { kind: 'pause' }
  | { kind: 'unpause' }
  | { kind: 'addToken'; token: string }
  | { kind: 'removeToken'; token: string }
  | { kind: 'rotateOracle'; oracle: string }
  | { kind: 'transferAdmin'; admin: string };

function actionLabel(a: PendingAction): string {
  switch (a.kind) {
    case 'pause': return 'Pause contract';
    case 'unpause': return 'Unpause contract';
    case 'addToken': return `Add token: ${a.token}`;
    case 'removeToken': return `Remove token: ${a.token}`;
    case 'rotateOracle': return `Rotate oracle to: ${a.oracle}`;
    case 'transferAdmin': return `Transfer admin to: ${a.admin}`;
  }
}

export function AdminPanel({ wallet }: Props) {
  const admin = useAdminContract(wallet.publicKey, wallet.type);

  const [pending, setPending] = useState<PendingAction | null>(null);
  const [tokenInput, setTokenInput] = useState('');
  const [oracleInput, setOracleInput] = useState('');
  const [newAdminInput, setNewAdminInput] = useState('');
  const [actionLog, setActionLog] = useState<string[]>([]);

  function log(msg: string) {
    setActionLog(prev => [`${new Date().toISOString()}  ${msg}`, ...prev].slice(0, 50));
  }

  async function execute(action: PendingAction) {
    let ok = false;
    switch (action.kind) {
      case 'pause':        ok = await admin.pause(); break;
      case 'unpause':      ok = await admin.unpause(); break;
      case 'addToken':     ok = await admin.addToken(action.token); break;
      case 'removeToken':  ok = await admin.removeToken(action.token); break;
      case 'rotateOracle': ok = await admin.rotateOracle(action.oracle); break;
      case 'transferAdmin': ok = await admin.transferAdmin(action.admin); break;
    }
    if (ok) log(`✓ ${actionLabel(action)}`);
    else log(`✗ ${actionLabel(action)}: ${admin.actionError ?? 'unknown error'}`);
    setPending(null);
  }

  if (!wallet.connected) {
    return (
      <main aria-label="Admin Panel">
        <h1>Admin Panel</h1>
        <p>Connect your admin wallet to continue.</p>
        <div role="region" aria-label="Connect wallet">
          <button type="button" onClick={() => wallet.connect('freighter')}>Connect Freighter</button>
          <button type="button" onClick={() => wallet.connect('albedo')}>Connect Albedo</button>
          {wallet.error && <p role="alert">{wallet.error}</p>}
        </div>
      </main>
    );
  }

  if (admin.loading) return <p aria-live="polite">Loading contract state…</p>;

  if (!admin.isAdmin) {
    return (
      <main aria-label="Admin Panel">
        <h1>Admin Panel</h1>
        <p role="alert">
          Connected wallet <code>{wallet.publicKey}</code> is not the contract admin.
        </p>
        <button type="button" onClick={wallet.disconnect}>Disconnect</button>
      </main>
    );
  }

  return (
    <main aria-label="Admin Panel">
      <h1>Admin Panel</h1>

      {pending && (
        <ConfirmDialog
          title="Confirm Action"
          message={actionLabel(pending)}
          onConfirm={() => execute(pending)}
          onCancel={() => setPending(null)}
        />
      )}

      {/* ── Status ─────────────────────────────────────────────── */}
      <section aria-labelledby="status-heading">
        <h2 id="status-heading">Contract Status</h2>
        <dl>
          <dt>Admin</dt>
          <dd>{admin.admin ?? '—'}</dd>
          <dt>Oracle</dt>
          <dd>{admin.oracle ?? '—'}</dd>
          <dt>Paused</dt>
          <dd aria-label={`Contract is ${admin.paused ? 'paused' : 'active'}`}>
            {admin.paused === null ? '—' : admin.paused ? 'Yes' : 'No'}
          </dd>
        </dl>
        <button type="button" onClick={admin.refresh} disabled={admin.loading}>
          Refresh
        </button>
      </section>

      {/* ── Pause / Unpause ────────────────────────────────────── */}
      <section aria-labelledby="pause-heading">
        <h2 id="pause-heading">Pause / Unpause</h2>
        {admin.paused
          ? <button type="button" onClick={() => setPending({ kind: 'unpause' })}>Unpause Contract</button>
          : <button type="button" onClick={() => setPending({ kind: 'pause' })}>Pause Contract</button>}
      </section>

      {/* ── Token Allowlist ────────────────────────────────────── */}
      <section aria-labelledby="token-heading">
        <h2 id="token-heading">Token Allowlist</h2>
        <div>
          <label htmlFor="token-input">Token address</label>
          <input
            id="token-input"
            type="text"
            value={tokenInput}
            onChange={e => setTokenInput(e.target.value)}
            placeholder="G…"
          />
          <button
            type="button"
            disabled={!tokenInput.trim()}
            onClick={() => { setPending({ kind: 'addToken', token: tokenInput.trim() }); }}
          >
            Add Token
          </button>
          <button
            type="button"
            disabled={!tokenInput.trim()}
            onClick={() => { setPending({ kind: 'removeToken', token: tokenInput.trim() }); }}
          >
            Remove Token
          </button>
        </div>
      </section>

      {/* ── Oracle Rotation ────────────────────────────────────── */}
      <section aria-labelledby="oracle-heading">
        <h2 id="oracle-heading">Oracle Rotation</h2>
        <div>
          <label htmlFor="oracle-input">New oracle address</label>
          <input
            id="oracle-input"
            type="text"
            value={oracleInput}
            onChange={e => setOracleInput(e.target.value)}
            placeholder="G…"
          />
          <p>
            <strong>Current oracle:</strong> {admin.oracle ?? '—'}
          </p>
          <p>
            <strong>New oracle preview:</strong> {oracleInput || '—'}
          </p>
          <button
            type="button"
            disabled={!oracleInput.trim()}
            onClick={() => { setPending({ kind: 'rotateOracle', oracle: oracleInput.trim() }); }}
          >
            Rotate Oracle
          </button>
        </div>
      </section>

      {/* ── Transfer Admin ─────────────────────────────────────── */}
      <section aria-labelledby="transfer-heading">
        <h2 id="transfer-heading">Transfer Admin</h2>
        <div>
          <label htmlFor="new-admin-input">New admin address</label>
          <input
            id="new-admin-input"
            type="text"
            value={newAdminInput}
            onChange={e => setNewAdminInput(e.target.value)}
            placeholder="G…"
          />
          <button
            type="button"
            disabled={!newAdminInput.trim()}
            onClick={() => { setPending({ kind: 'transferAdmin', admin: newAdminInput.trim() }); }}
          >
            Transfer Admin
          </button>
        </div>
      </section>

      {/* ── Action Log ─────────────────────────────────────────── */}
      <section aria-labelledby="log-heading">
        <h2 id="log-heading">Action Log</h2>
        {actionLog.length === 0
          ? <p>No actions yet.</p>
          : <ul>{actionLog.map((entry, i) => <li key={i}>{entry}</li>)}</ul>}
      </section>

      {admin.actionError && <p role="alert">{admin.actionError}</p>}
      {admin.actionLoading && <p aria-live="polite">Processing…</p>}
    </main>
  );
}
