import { isConnected, getAddress, signTransaction } from '@stellar/freighter-api';
import type { SignResult } from './types';

export async function freighterIsAvailable(): Promise<boolean> {
  try {
    const result = await isConnected();
    return result.isConnected;
  } catch {
    return false;
  }
}

export async function freighterGetPublicKey(): Promise<string> {
  const result = await getAddress();
  if (result.error) throw new Error(result.error.message);
  return result.address;
}

export async function freighterSign(xdr: string, network: string): Promise<SignResult> {
  const result = await signTransaction(xdr, { networkPassphrase: network });
  if (result.error) throw new Error(result.error.message);
  return { signedXdr: result.signedTxXdr };
}
