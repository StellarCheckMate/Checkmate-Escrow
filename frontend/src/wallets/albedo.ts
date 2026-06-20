import albedo from '@albedo-link/intent';
import type { SignResult } from './types';

export function albedoIsAvailable(): boolean {
  return typeof window !== 'undefined';
}

export async function albedoGetPublicKey(): Promise<string> {
  const result = await albedo.publicKey({});
  return result.pubkey;
}

export async function albedoSign(xdr: string, network: string): Promise<SignResult> {
  const result = await albedo.tx({ xdr, network, submit: false });
  return { signedXdr: result.signed_envelope_xdr };
}
