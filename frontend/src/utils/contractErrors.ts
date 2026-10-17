/**
 * Mapping of escrow/oracle contract error codes to human-readable,
 * user-facing messages.
 *
 * Error codes and names are sourced from `docs/error-codes.md`.
 * Messages must never embed raw RPC payloads or internal match data.
 */

/** Human-readable message for an unrecognized contract error code. */
export const UNKNOWN_CONTRACT_ERROR_MESSAGE =
  'Something went wrong while processing the contract call. Please try again.';

/**
 * Contract error code -> user-facing message.
 *
 * Keys are the numeric error codes as defined in `docs/error-codes.md`.
 */
export const CONTRACT_ERROR_MESSAGES: Readonly<Record<number, string>> = {
  // Escrow contract errors
  1: 'The contract received invalid input. Please check your details and try again.',
  2: 'This operation is not allowed for the current contract state.',
  3: 'The escrow has not been funded yet. Please fund it before continuing.',
  4: 'You are not authorized to perform this action.',
  5: 'This action cannot be performed in the current state.',
  6: 'The requested amount is invalid.',
  7: 'The escrow has already been funded.',
  8: 'The escrow has already been completed.',
  9: 'The contract is currently paused. Please try again later.',
  10: 'The escrow has expired.',
  11: 'The deadline has not been reached yet.',
  12: 'The deadline has already passed.',
  13: 'The escrow was not found.',
  14: 'The participant was not found.',
  15: 'This participant has already been added.',
  16: 'The escrow is already in dispute.',
  17: 'The dispute has already been resolved.',
  18: 'The dispute has not been resolved yet.',
  19: 'The escrow cannot be cancelled in its current state.',
  20: 'The escrow cannot be released in its current state.',
  21: 'The escrow cannot be refunded in its current state.',
  22: 'Insufficient funds to complete this operation.',
  23: 'The fee amount is invalid.',
  24: 'The fee recipient is invalid.',
  25: 'The token is not supported.',
  26: 'The token transfer failed.',
  27: 'The token approval failed.',
  28: 'The allowance is insufficient.',
  29: 'The balance is insufficient.',
  30: 'The amount is below the minimum allowed.',
  31: 'The amount exceeds the maximum allowed.',
  32: 'The operation is not permitted at this time.',
  33: 'The signature is invalid.',
  34: 'The nonce has already been used.',
  35: 'The request has expired.',
  36: 'The request is not yet valid.',
  37: 'The caller is not the owner.',
  38: 'The caller is not an admin.',
  39: 'The caller is not a participant.',
  40: 'The caller is not the arbitrator.',
  41: 'The escrow is locked.',
  42: 'The escrow is not locked.',
  43: 'The escrow is already active.',
  44: 'The escrow is not active.',
  45: 'The upgrade was not found.',
  46: 'The upgrade has already been approved.',
  47: 'The upgrade has already been rejected.',
  48: 'The upgrade review period has not elapsed yet. Please wait before proceeding.',
  49: 'The upgrade review period has already elapsed.',
  50: 'The contract version is not supported.',

  // Oracle contract errors
  100: 'The oracle received invalid input. Please check your details and try again.',
  101: 'The oracle is not authorized to perform this action.',
  102: 'The price feed was not found.',
  103: 'The price feed is stale.',
  104: 'The price data is invalid.',
  105: 'The oracle is currently paused. Please try again later.',
  106: 'The oracle request has expired.',
  107: 'The oracle request is not yet valid.',
  108: 'The oracle response is invalid.',
  109: 'The oracle has already responded to this request.',
  110: 'The oracle request was not found.',
};

/**
 * Contract error name -> user-facing message.
 *
 * Useful when the error is surfaced by name rather than by numeric code.
 */
export const CONTRACT_ERROR_NAME_MESSAGES: Readonly<Record<string, string>> = {
  InvalidInput: 'The contract received invalid input. Please check your details and try again.',
  InvalidState: 'This action cannot be performed in the current state.',
  NotFunded: 'The escrow has not been funded yet. Please fund it before continuing.',
  Unauthorized: 'You are not authorized to perform this action.',
  ContractPaused: 'The contract is currently paused. Please try again later.',
  AlreadyFunded: 'The escrow has already been funded.',
  AlreadyCompleted: 'The escrow has already been completed.',
  Expired: 'The escrow has expired.',
  DeadlineNotReached: 'The deadline has not been reached yet.',
  DeadlinePassed: 'The deadline has already passed.',
  NotFound: 'The requested item was not found.',
  AlreadyExists: 'This item has already been added.',
  InDispute: 'The escrow is already in dispute.',
  DisputeResolved: 'The dispute has already been resolved.',
  DisputeNotResolved: 'The dispute has not been resolved yet.',
  InsufficientFunds: 'Insufficient funds to complete this operation.',
  InvalidAmount: 'The requested amount is invalid.',
  InvalidFee: 'The fee amount is invalid.',
  InvalidToken: 'The token is not supported.',
  TransferFailed: 'The token transfer failed.',
  ApprovalFailed: 'The token approval failed.',
  InsufficientAllowance: 'The allowance is insufficient.',
  InsufficientBalance: 'The balance is insufficient.',
  InvalidSignature: 'The signature is invalid.',
  NonceAlreadyUsed: 'The nonce has already been used.',
  RequestExpired: 'The request has expired.',
  RequestNotYetValid: 'The request is not yet valid.',
  NotOwner: 'The caller is not the owner.',
  NotAdmin: 'The caller is not an admin.',
  NotParticipant: 'The caller is not a participant.',
  NotArbitrator: 'The caller is not the arbitrator.',
  Locked: 'The escrow is locked.',
  NotLocked: 'The escrow is not locked.',
  AlreadyActive: 'The escrow is already active.',
  NotActive: 'The escrow is not active.',
  UpgradeNotFound: 'The upgrade was not found.',
  UpgradeAlreadyApproved: 'The upgrade has already been approved.',
  UpgradeAlreadyRejected: 'The upgrade has already been rejected.',
  UpgradeReviewPeriodNotElapsed:
    'The upgrade review period has not elapsed yet. Please wait before proceeding.',
  UpgradeReviewPeriodElapsed: 'The upgrade review period has already elapsed.',
  UnsupportedVersion: 'The contract version is not supported.',
  PriceFeedNotFound: 'The price feed was not found.',
  StalePrice: 'The price feed is stale.',
  InvalidPrice: 'The price data is invalid.',
  OraclePaused: 'The oracle is currently paused. Please try again later.',
  OracleRequestExpired: 'The oracle request has expired.',
  OracleRequestNotYetValid: 'The oracle request is not yet valid.',
  InvalidResponse: 'The oracle response is invalid.',
  AlreadyResponded: 'The oracle has already responded to this request.',
  OracleRequestNotFound: 'The oracle request was not found.',
};

/**
 * Returns the user-facing message for a numeric contract error code.
 * Falls back to the generic unknown-error message for unrecognized codes.
 */
export function getContractErrorMessage(code: number | null | undefined): string {
  if (code === null || code === undefined || !Number.isFinite(code)) {
    return UNKNOWN_CONTRACT_ERROR_MESSAGE;
  }
  return CONTRACT_ERROR_MESSAGES[code] ?? UNKNOWN_CONTRACT_ERROR_MESSAGE;
}

/**
 * Returns the user-facing message for a contract error name.
 * Falls back to the generic unknown-error message for unrecognized names.
 */
export function getContractErrorNameMessage(name: string | null | undefined): string {
  if (!name) {
    return UNKNOWN_CONTRACT_ERROR_MESSAGE;
  }
  return CONTRACT_ERROR_NAME_MESSAGES[name] ?? UNKNOWN_CONTRACT_ERROR_MESSAGE;
}

/**
 * Matches strings like `Error(Contract, #5)` or `Error(Contract, #48)`.
 * The capture group holds the numeric error code.
 */
const CONTRACT_ERROR_PATTERN = /Error\(\s*Contract\s*,\s*#(\d+)\s*\)/;

/**
 * Parses a contract error string (e.g. `Error(Contract, #5)`) and returns
 * the mapped user-facing message. Falls back to the generic unknown-error
 * message when the string cannot be parsed or the code is unrecognized.
 */
export function parseContractError(error: unknown): string {
  if (error === null || error === undefined) {
    return UNKNOWN_CONTRACT_ERROR_MESSAGE;
  }

  const raw =
    typeof error === 'string'
      ? error
      : typeof (error as { message?: unknown }).message === 'string'
        ? ((error as { message: string }).message)
        : '';

  if (!raw) {
    return UNKNOWN_CONTRACT_ERROR_MESSAGE;
  }

  const match = raw.match(CONTRACT_ERROR_PATTERN);
  if (match) {
    const code = Number.parseInt(match[1], 10);
    return getContractErrorMessage(code);
  }

  // Attempt to match a known error name embedded in the string.
  for (const name of Object.keys(CONTRACT_ERROR_NAME_MESSAGES)) {
    if (raw.includes(name)) {
      return getContractErrorNameMessage(name);
    }
  }

  return UNKNOWN_CONTRACT_ERROR_MESSAGE;
}