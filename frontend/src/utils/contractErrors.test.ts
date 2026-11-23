import { expect, it } from 'vitest';
import { getContractErrorMessage, mapContractError, parseContractError } from './contractErrors';

it('includes an unknown numeric code without exposing the RPC message', () => {
  expect(getContractErrorMessage(987)).toContain('987');
  const message = parseContractError('RPC details: Error(Contract, #987)');
  expect(message).toContain('987');
  expect(message).not.toContain('RPC details');
});

it('maps a known code to its documented message', () => {
  expect(getContractErrorMessage(4)).toBe('You are not authorized to perform this action.');
  expect(mapContractError('Error(Contract, #5)')).toBe('This action cannot be performed in the current state.');
  expect(mapContractError('NotFunded')).toBe('The escrow has not been funded yet. Please fund it before continuing.');
});
