import { describe, it, expect } from 'vitest';
import {
  UNKNOWN_CONTRACT_ERROR_MESSAGE,
  CONTRACT_ERROR_MESSAGES,
  CONTRACT_ERROR_NAME_MESSAGES,
  getContractErrorMessage,
  getContractErrorNameMessage,
  parseContractError,
  mapContractError,
} from './contractErrors';

describe('contractErrors', () => {
  describe('getContractErrorMessage', () => {
    it('returns the mapped human-readable message for known error codes', () => {
      expect(getContractErrorMessage(1)).toBe(CONTRACT_ERROR_MESSAGES[1]);
      expect(getContractErrorMessage(5)).toBe(CONTRACT_ERROR_MESSAGES[5]);
      expect(getContractErrorMessage(100)).toBe(CONTRACT_ERROR_MESSAGES[100]);
    });

    it('includes the numeric error code in the message for unknown codes', () => {
      const code = 9999;
      const message = getContractErrorMessage(code);
      expect(message).toContain(String(code));
      expect(message).toBe(
        `Something went wrong while processing the contract call (error code ${code}). Please try again.`,
      );
    });

    it('asserts different unknown numeric codes appear in their respective messages', () => {
      expect(getContractErrorMessage(12345)).toContain('12345');
      expect(getContractErrorMessage(500)).toContain('500');
    });

    it('falls back to the generic unknown message when code is null or undefined', () => {
      expect(getContractErrorMessage(null)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
      expect(getContractErrorMessage(undefined)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
    });

    it('falls back to the generic unknown message when code is NaN or non-finite', () => {
      expect(getContractErrorMessage(Number.NaN)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
      expect(getContractErrorMessage(Number.POSITIVE_INFINITY)).toBe(
        UNKNOWN_CONTRACT_ERROR_MESSAGE,
      );
    });
  });

  describe('getContractErrorNameMessage', () => {
    it('returns the mapped human-readable message for known error names', () => {
      expect(getContractErrorNameMessage('InvalidState')).toBe(
        CONTRACT_ERROR_NAME_MESSAGES['InvalidState'],
      );
      expect(getContractErrorNameMessage('NotFunded')).toBe(
        CONTRACT_ERROR_NAME_MESSAGES['NotFunded'],
      );
    });

    it('falls back to the generic unknown message for unknown error names', () => {
      expect(getContractErrorNameMessage('NonExistentErrorName')).toBe(
        UNKNOWN_CONTRACT_ERROR_MESSAGE,
      );
      expect(getContractErrorNameMessage(null)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
      expect(getContractErrorNameMessage(undefined)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
    });
  });

  describe('parseContractError', () => {
    it('extracts known numeric code from Error(Contract, #code) string', () => {
      const message = parseContractError('Error(Contract, #5)');
      expect(message).toBe(CONTRACT_ERROR_MESSAGES[5]);
    });

    it('extracts unknown numeric code and includes the code in the returned message', () => {
      const message = parseContractError('Error(Contract, #9999)');
      expect(message).toContain('9999');
      expect(message).toBe(
        'Something went wrong while processing the contract call (error code 9999). Please try again.',
      );
    });

    it('handles Error object containing contract error pattern in message', () => {
      const error = new Error('HostError: Error(Contract, #8888)');
      const message = parseContractError(error);
      expect(message).toContain('8888');
    });

    it('matches known error names embedded in string', () => {
      const message = parseContractError('Transaction aborted due to NotFunded condition');
      expect(message).toBe(CONTRACT_ERROR_NAME_MESSAGES['NotFunded']);
    });

    it('falls back to generic unknown message for unrecognized error strings without codes', () => {
      expect(parseContractError('Network connection timed out')).toBe(
        UNKNOWN_CONTRACT_ERROR_MESSAGE,
      );
      expect(parseContractError(null)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
      expect(parseContractError(undefined)).toBe(UNKNOWN_CONTRACT_ERROR_MESSAGE);
    });
  });

  describe('mapContractError alias', () => {
    it('is exported and behaves identically to parseContractError', () => {
      expect(mapContractError).toBe(parseContractError);
      expect(mapContractError('Error(Contract, #7777)')).toContain('7777');
    });
  });
});
