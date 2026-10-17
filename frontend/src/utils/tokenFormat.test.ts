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

  it('handles large BigInt values above 2^53 without precision loss', () => {
    // 2^60 is well above 2^53 (JavaScript number limit)
    const largeValue = 1152921504606846976n; // 2^60
    expect(formatTokenAmount(largeValue, 7)).toBe('115292150460.6846976');
  });

  it('handles very large values that exceed number precision', () => {
    // 900 billion XLM in stroops (7 decimals)
    const nineHundredBillion = 9000000000000000000n; // 900B XLM
    expect(formatTokenAmount(nineHundredBillion, 7)).toBe('900000000000');
  });

  it('handles 18-decimal token with large amounts', () => {
    // i128 max is around 2^127, test with a large but realistic amount
    const largeAmount = 999999999999999999999999999n; // 27 nines
    expect(formatTokenAmount(largeAmount, 18)).toBe('999999999.999999999999999999');
  });

  it('parses large string numbers correctly', () => {
    const largeString = '9999999999999999999999';
    expect(formatTokenAmount(largeString, 10)).toBe('999999999999.9999999999');
  });

  it('handles zero with large decimals', () => {
    expect(formatTokenAmount(0n, 20)).toBe('0');
  });

  it('handles negative large BigInt values', () => {
    const negLarge = -1152921504606846976n; // -2^60
    expect(formatTokenAmount(negLarge, 7)).toBe('-115292150460.6846976');
  });

  describe('typical token decimals', () => {
    it('formats a 7-decimal token balance', () => {
      expect(formatTokenAmount(123_456_789, 7)).toBe('12.3456789');
    });

    it('formats a 6-decimal token balance', () => {
      expect(formatTokenAmount(123_456_789, 6)).toBe('123.456789');
    });

    it('formats an 18-decimal token balance', () => {
      expect(formatTokenAmount(123_456_789_000_000_000_000n, 18)).toBe('123.456789');
    });
  });

  describe('edge cases', () => {
    it('formats a zero balance as "0" for any decimals', () => {
      expect(formatTokenAmount(0, 7)).toBe('0');
      expect(formatTokenAmount(0n, 6)).toBe('0');
      expect(formatTokenAmount('0', 18)).toBe('0');
    });

    it('formats whole amounts without a fraction part', () => {
      expect(formatTokenAmount(10_000_000, 7)).toBe('1');
      expect(formatTokenAmount(1_000_000, 6)).toBe('1');
      expect(formatTokenAmount(1_000_000_000_000_000_000n, 18)).toBe('1');
    });

    it('formats amounts smaller than one unit', () => {
      expect(formatTokenAmount(1, 7)).toBe('0.0000001');
      expect(formatTokenAmount(1, 6)).toBe('0.000001');
      expect(formatTokenAmount(1n, 18)).toBe('0.000000000000000001');
    });

    it('formats negative amounts smaller than one unit', () => {
      expect(formatTokenAmount(-1, 7)).toBe('-0.0000001');
      expect(formatTokenAmount(-1n, 18)).toBe('-0.000000000000000001');
    });
  });
});
