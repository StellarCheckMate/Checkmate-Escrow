import { useState, useCallback, useEffect } from 'react';
import type { WalletType } from '../wallets/types';

const CONTRACT_ID = import.meta.env.VITE_CONTRACT_ESCROW ?? '';
const RPC_URL = import.meta.env.VITE_STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org';

export interface AdminState {
  admin: string | null;
  oracle: string | null;
  paused: boolean | null;
  loading: boolean;
  error: string | null;
}

async function callView(method: string): Promise<unknown> {
  const body = {
    jsonrpc: '2.0',
    id: 1,
    method: 'simulateTransaction',
    params: { transaction: buildInvokeTx(method, []) },
  };
  const res = await fetch(RPC_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`RPC error: ${res.statusText}`);
  const json = (await res.json()) as { result?: { results?: Array<{ xdr: string }> }; error?: { message: string } };
  if (json.error) throw new Error(json.error.message);
  return json.result?.results?.[0]?.xdr ?? null;
}

// Stub: builds a minimal invoke-host-function transaction XDR.
// In production this would use @stellar/stellar-sdk ContractSpec + TransactionBuilder.
function buildInvokeTx(_method: string, _args: unknown[]): string {
  return '';
}

export function useAdminContract(walletPublicKey: string | null, _walletType: WalletType | null) {
  const [state, setState] = useState<AdminState>({
    admin: null,
    oracle: null,
    paused: null,
    loading: false,
    error: null,
  });
  const [actionLoading, setActionLoading] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const fetchAdminState = useCallback(async () => {
    if (!CONTRACT_ID) return;
    setState(s => ({ ...s, loading: true, error: null }));
    try {
      // In a real integration these would decode SCVal XDR returned by the RPC.
      // Here we call the view functions and return placeholders so the UI wires up correctly.
      const [adminXdr, oracleXdr, pausedXdr] = await Promise.all([
        callView('get_admin').catch(() => null),
        callView('get_oracle').catch(() => null),
        callView('is_paused').catch(() => null),
      ]);
      setState({
        admin: adminXdr ? String(adminXdr) : null,
        oracle: oracleXdr ? String(oracleXdr) : null,
        paused: pausedXdr === null ? null : pausedXdr === 'true' || pausedXdr === true,
        loading: false,
        error: null,
      });
    } catch (err) {
      setState(s => ({ ...s, loading: false, error: (err as Error).message }));
    }
  }, []);

  useEffect(() => {
    fetchAdminState();
  }, [fetchAdminState]);

  const isAdmin = walletPublicKey !== null && state.admin !== null && walletPublicKey === state.admin;

  async function invoke(method: string, args: unknown[]): Promise<boolean> {
    if (!isAdmin) {
      setActionError('Not authorized: connected wallet is not the contract admin.');
      return false;
    }
    setActionLoading(true);
    setActionError(null);
    try {
      // In production: build + sign + submit transaction via stellar-sdk.
      // This stub simulates a successful call for UI/test purposes.
      void method; void args;
      await new Promise(r => setTimeout(r, 300)); // simulate async
      await fetchAdminState();
      return true;
    } catch (err) {
      setActionError((err as Error).message);
      return false;
    } finally {
      setActionLoading(false);
    }
  }

  const pause = () => invoke('pause', []);
  const unpause = () => invoke('unpause', []);
  const addToken = (token: string) => invoke('add_allowed_token', [token]);
  const removeToken = (token: string) => invoke('remove_allowed_token', [token]);
  const rotateOracle = (newOracle: string) => invoke('update_oracle', [newOracle]);
  const transferAdmin = (newAdmin: string) => invoke('transfer_admin', [newAdmin]);

  return {
    ...state,
    isAdmin,
    actionLoading,
    actionError,
    refresh: fetchAdminState,
    pause,
    unpause,
    addToken,
    removeToken,
    rotateOracle,
    transferAdmin,
  };
}
