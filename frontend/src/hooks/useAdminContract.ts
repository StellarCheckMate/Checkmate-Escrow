import { useState, useCallback, useEffect } from 'react';
import {
  TransactionBuilder,
  Account,
  Networks,
  Contract,
  Address,
  nativeToScVal,
  xdr,
} from '@stellar/stellar-sdk';
import { freighterSign } from '../wallets/freighter';
import { albedoSign } from '../wallets/albedo';
import type { WalletType } from '../wallets/types';

const CONTRACT_ID = import.meta.env.VITE_CONTRACT_ESCROW ?? '';
const RPC_URL = import.meta.env.VITE_STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org';
const NETWORK = import.meta.env.VITE_STELLAR_NETWORK === 'mainnet'
  ? Networks.PUBLIC
  : Networks.TESTNET;

export interface AdminState {
  admin: string | null;
  oracle: string | null;
  paused: boolean | null;
  protocolConfig: ProtocolConfigForm | null;
  loading: boolean;
  error: string | null;
}

function decodeXdrBuffer(xdrBase64: string): Uint8Array {
  const binaryStr = atob(xdrBase64);
  const bytes = new Uint8Array(binaryStr.length);
  for (let i = 0; i < binaryStr.length; i++) {
    bytes[i] = binaryStr.charCodeAt(i);
  }
  return bytes;
}

export function decodeAddress(scValXdr: string): string {
  try {
    const buffer = decodeXdrBuffer(scValXdr);
    const val = xdr.ScVal.fromXDR(buffer);
    // Check if this is an address by examining the switch type
    if (val.switch().name === 'scvAddress') {
      const addr = val.address();
      if (addr) {
        return Address.fromScAddress(addr).toString();
      }
    }
    throw new Error('Not an address SCVal');
  } catch (err) {
    throw new Error(`Failed to decode address: ${(err as Error).message}`, { cause: err });
  }
}

export function decodeBoolean(scValXdr: string): boolean {
  try {
    const buffer = decodeXdrBuffer(scValXdr);
    const val = xdr.ScVal.fromXDR(buffer);
    // Check if this is a boolean by examining the switch type
    if (val.switch().name === 'scvBool') {
      return val.b()?.valueOf() ?? false;
    }
    throw new Error('Not a boolean SCVal');
  } catch (err) {
    throw new Error(`Failed to decode boolean: ${(err as Error).message}`, { cause: err });
  }
}

export async function buildInvokeTx(walletPublicKey: string, method: string, args: unknown[]): Promise<string> {
  // Fetch current account info for sequence number
  const accountResponse = await fetch(RPC_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method: 'getAccount',
      params: [walletPublicKey],
    }),
  });

  if (!accountResponse.ok) throw new Error('Failed to fetch account info');
  const accountData = (await accountResponse.json()) as { result?: { sequence: string }; error?: { message: string } };
  if (accountData.error) throw new Error(accountData.error.message);

  const sequence = accountData.result?.sequence ?? '0';
  const account = new Account(walletPublicKey, sequence);

  const contract = new Contract(CONTRACT_ID);

  const argsXdr = args.map(arg => {
    if (typeof arg === 'string' && arg.startsWith('G')) {
      return new Address(arg).toScVal();
    }
    return nativeToScVal(arg);
  });

  const txBuilder = new TransactionBuilder(account, {
    fee: '100',
    networkPassphrase: NETWORK,
  });

  const op = contract.call(method, ...argsXdr);
  txBuilder.addOperation(op);

  const tx = txBuilder.build();
  return tx.toXDR();
}

export async function callView(walletPublicKey: string, method: string): Promise<string | null> {
  const xdrTx = await buildInvokeTx(walletPublicKey, method, []);
  const body = {
    jsonrpc: '2.0',
    id: 1,
    method: 'simulateTransaction',
    params: { transaction: xdrTx },
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

export interface ProtocolConfigForm {
  vestingDurationSeconds: number | null;
  cancellationFeeBasisPoints: number | null;
  treasury: string | null;
  stablecoinOnlyMode: boolean | null;
  matchTimeoutSeconds: number | null;
  protocolFeeBps: number | null;
  feeRecipient: string | null;
  minimumStake: string | null;
}

function decodeU32(scVal: xdr.ScVal): number | null {
  try {
    if (scVal.switch().name === 'scvU32') return scVal.u32();
    if (scVal.switch().name === 'scvI128' || scVal.switch().name === 'scvU128') {
      // Large numeric fields (e.g. stake amounts) - best-effort decode.
      return Number(scValToBigIntString(scVal));
    }
    return null;
  } catch {
    return null;
  }
}

function scValToBigIntString(scVal: xdr.ScVal): string {
  try {
    const parts = scVal.switch().name === 'scvI128' ? scVal.i128() : scVal.u128();
    const hi = BigInt(parts.hi().toString());
    const lo = BigInt(parts.lo().toString());
    return ((hi << BigInt(64)) + lo).toString();
  } catch {
    return '0';
  }
}

/**
 * Decodes a `ProtocolConfig` struct returned as an XDR-encoded `ScVal` map
 * (Soroban serializes `#[contracttype]` structs with named fields as an
 * `ScMap` keyed by symbol). Missing/unrecognized fields decode to `null`
 * rather than throwing, so the admin form can still render partial data.
 */
export function decodeProtocolConfig(xdrBase64: string): ProtocolConfigForm {
  const empty: ProtocolConfigForm = {
    vestingDurationSeconds: null,
    cancellationFeeBasisPoints: null,
    treasury: null,
    stablecoinOnlyMode: null,
    matchTimeoutSeconds: null,
    protocolFeeBps: null,
    feeRecipient: null,
    minimumStake: null,
  };
  try {
    const buffer = decodeXdrBuffer(xdrBase64);
    const val = xdr.ScVal.fromXDR(buffer);
    if (val.switch().name !== 'scvMap') return empty;
    const entries = val.map() ?? [];
    const result = { ...empty };
    for (const entry of entries) {
      const key = entry.key();
      if (key.switch().name !== 'scvSymbol') continue;
      const fieldName = key.sym().toString();
      const value = entry.val();
      switch (fieldName) {
        case 'vesting_duration_seconds':
          result.vestingDurationSeconds = decodeU32(value);
          break;
        case 'cancellation_fee_basis_points':
          result.cancellationFeeBasisPoints = decodeU32(value);
          break;
        case 'treasury':
          result.treasury = value.switch().name === 'scvAddress'
            ? Address.fromScAddress(value.address()).toString()
            : null;
          break;
        case 'stablecoin_only_mode':
          result.stablecoinOnlyMode = value.switch().name === 'scvBool' ? value.b() : null;
          break;
        case 'match_timeout_seconds':
          result.matchTimeoutSeconds = decodeU32(value);
          break;
        case 'protocol_fee_bps':
          result.protocolFeeBps = decodeU32(value);
          break;
        case 'fee_recipient':
          result.feeRecipient = value.switch().name === 'scvAddress'
            ? Address.fromScAddress(value.address()).toString()
            : null;
          break;
        case 'minimum_stake':
          result.minimumStake = value.switch().name === 'scvI128' || value.switch().name === 'scvU128'
            ? scValToBigIntString(value)
            : null;
          break;
        default:
          break;
      }
    }
    return result;
  } catch {
    return empty;
  }
}

export function isContractPausedError(error: unknown): boolean {
  if (!error) return false;
  const msg = typeof error === 'string' ? error : (error as Error).message || String(error);
  return (
    msg.includes('ContractPaused') ||
    msg.includes('#9') ||
    msg.includes('Error(Contract, #9)') ||
    msg.toLowerCase().includes('contract paused') ||
    msg.toLowerCase().includes('contract is paused')
  );
}

export function useAdminContract(walletPublicKey: string | null, walletType: WalletType | null) {
  const [state, setState] = useState<AdminState>({
    admin: null,
    oracle: null,
    paused: null,
    protocolConfig: null,
    loading: false,
    error: null,
  });
  const [actionLoading, setActionLoading] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const fetchAdminState = useCallback(async () => {
    if (!CONTRACT_ID || !walletPublicKey) return;
    setState(s => ({ ...s, loading: true, error: null }));
    try {
      const [adminXdr, oracleXdr, pausedXdr, protocolConfigXdr] = await Promise.all([
        callView(walletPublicKey, 'get_admin').catch(() => null),
        callView(walletPublicKey, 'get_oracle').catch(() => null),
        callView(walletPublicKey, 'is_paused').catch(() => null),
        callView(walletPublicKey, 'get_protocol_config').catch(() => null),
      ]);

      let admin: string | null = null;
      let oracle: string | null = null;
      let paused: boolean | null = null;
      let protocolConfig: ProtocolConfigForm | null = null;

      if (adminXdr) {
        try {
          admin = decodeAddress(adminXdr);
        } catch {
          admin = null;
        }
      }

      if (oracleXdr) {
        try {
          oracle = decodeAddress(oracleXdr);
        } catch {
          oracle = null;
        }
      }

      if (pausedXdr) {
        try {
          paused = decodeBoolean(pausedXdr);
        } catch {
          paused = null;
        }
      }

      if (protocolConfigXdr) {
        try {
          protocolConfig = decodeProtocolConfig(protocolConfigXdr);
        } catch {
          protocolConfig = null;
        }
      }

      setState({
        admin,
        oracle,
        paused,
        protocolConfig,
        loading: false,
        error: null,
      });
    } catch (err) {
      setState(s => ({ ...s, loading: false, error: (err as Error).message }));
    }
  }, [walletPublicKey]);

  useEffect(() => {
    fetchAdminState();
  }, [fetchAdminState]);

  const isAdmin = walletPublicKey !== null && state.admin !== null && walletPublicKey === state.admin;

  async function invoke(method: string, args: unknown[]): Promise<boolean> {
    if (!isAdmin || !walletPublicKey || !walletType) {
      setActionError('Not authorized: connected wallet is not the contract admin.');
      return false;
    }
    setActionLoading(true);
    setActionError(null);
    try {
      const xdrTx = await buildInvokeTx(walletPublicKey, method, args);

      const signResult = walletType === 'freighter'
        ? await freighterSign(xdrTx, NETWORK)
        : await albedoSign(xdrTx, 'testnet');

      const signedXdr = signResult.signedXdr;

      const submitRes = await fetch(RPC_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: 1,
          method: 'sendTransaction',
          params: [signedXdr],
        }),
      });

      if (!submitRes.ok) throw new Error(`RPC submission error: ${submitRes.statusText}`);
      const submitData = (await submitRes.json()) as { result?: { status: string }; error?: { message: string } };
      if (submitData.error) throw new Error(submitData.error.message);

      await fetchAdminState();
      return true;
    } catch (err) {
      if (isContractPausedError(err)) {
        setState(s => ({ ...s, paused: true }));
        setActionError('Contract is currently paused. Actions are disabled until unpaused.');
      } else {
        setActionError((err as Error).message);
      }
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
