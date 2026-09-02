import {
  Account,
  BASE_FEE,
  Contract,
  Keypair,
  Networks,
  SorobanRpc,
  TransactionBuilder,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

const SOROBAN_RPC_URL = import.meta.env.VITE_SOROBAN_RPC_URL ?? 'https://soroban-testnet.stellar.org';
const NETWORK_PASSPHRASE = import.meta.env.VITE_STELLAR_NETWORK_PASSPHRASE ?? Networks.TESTNET;

const decimalsCache = new Map<string, number>();

/**
 * Formats a raw on-chain integer amount (stroops, or the smallest unit of
 * any Stellar token) into a human-readable decimal string using the
 * token's `decimals` value.
 *
 * @example formatTokenAmount(1_000_000, 7) === "0.1"
 * @example formatTokenAmount("2500000000", 7) === "250"
 */
export function formatTokenAmount(rawAmount: string | number | bigint, decimals: number): string {
  const raw = BigInt(rawAmount);
  if (decimals <= 0) return raw.toString();

  const negative = raw < 0n;
  const abs = negative ? -raw : raw;
  const divisor = 10n ** BigInt(decimals);
  const whole = abs / divisor;
  const fraction = (abs % divisor).toString().padStart(decimals, '0').replace(/0+$/, '');

  const result = fraction.length > 0 ? `${whole}.${fraction}` : whole.toString();
  return negative ? `-${result}` : result;
}

/**
 * Reads the `decimals()` value from a Soroban token contract (SEP-41 /
 * Stellar Asset Contract interface) via a read-only simulation, since
 * `decimals` never changes for a given token. Results are cached per
 * contract id for the lifetime of the page.
 */
export async function fetchTokenDecimals(
  tokenContractId: string,
  rpcUrl: string = SOROBAN_RPC_URL,
): Promise<number> {
  const cached = decimalsCache.get(tokenContractId);
  if (cached !== undefined) return cached;

  const server = new SorobanRpc.Server(rpcUrl);
  const contract = new Contract(tokenContractId);
  // Simulation-only source account: sequence number is irrelevant since the
  // transaction is never submitted, only simulated to read `decimals()`.
  const simulationAccount = new Account(Keypair.random().publicKey(), '0');

  const tx = new TransactionBuilder(simulationAccount, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call('decimals'))
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (!SorobanRpc.Api.isSimulationSuccess(sim) || !sim.result) {
    throw new Error(`Failed to fetch decimals for token ${tokenContractId}`);
  }

  const decimals = scValToNative(sim.result.retval as xdr.ScVal) as number;
  decimalsCache.set(tokenContractId, decimals);
  return decimals;
}
