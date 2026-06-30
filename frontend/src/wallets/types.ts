export type WalletType = 'freighter' | 'albedo';

export interface WalletState {
  type: WalletType | null;
  publicKey: string | null;
  connected: boolean;
  error: string | null;
}

export interface SignResult {
  signedXdr: string;
}
