import { describe, it, expect } from 'vitest';
import { formatTokenAmount } from './tokenFormat';

describe('formatTokenAmount', () => {
  it('formats 1_000_000 stroops of a 7-decimal token as "0.1"', () => {
    expect(formatTokenAmount(1_000_000, 7)).toBe('0.1');
  });

  it('formats a whole-number amount with no remainder', () => {
    expect(formatTokenAmount(2_500_000_000, 7)).toBe('250');
  });

  it('formats string input the same as numeric input', () => {
    expect(formatTokenAmount('1000000', 7)).toBe('0.1');
  });

  it('handles 0 decimals by returning the raw integer', () => {
    expect(formatTokenAmount(42, 0)).toBe('42');
  });

  it('handles negative amounts', () => {
    expect(formatTokenAmount(-1_000_000, 7)).toBe('-0.1');
  });

  it('trims trailing zero fraction digits', () => {
    expect(formatTokenAmount(1_500_000_0, 7)).toBe('1.5');
  });
});
