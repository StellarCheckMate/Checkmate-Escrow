import React, { useId } from 'react';

export type StakeAmountInputProps = {
  /** HTML id for the input element – needed for label association */
  id?: string;
  /** Symbol of the token to display as a suffix */
  tokenSymbol: string;
  /** Current string value of the input */
  value: string;
  /** Change handler */
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  /** Minimum allowed amount (default 0) */
  min?: number;
  /** Minimum stake amount from contract protocol config */
  minimum_stake?: number;
  /** Maximum stake amount from contract protocol config */
  maximum_stake?: number;
  /** Alias max */
  max?: number;
};

/**
 * Input component for entering a staking amount.
 * Displays the token symbol as a suffix and performs inline validation against
 * contract protocol config bounds (minimum_stake, maximum_stake) or standard min/max.
 * Uses a plain text input (type="text") so it works with the test suite's
 * `getByRole('textbox')` query while still allowing numeric validation.
 */
export const StakeAmountInput: React.FC<StakeAmountInputProps> = ({
  id,
  tokenSymbol,
  value,
  onChange,
  min = 0,
  minimum_stake,
  maximum_stake,
  max,
}) => {
  const errorId = useId();
  const numericValue = Number(value);
  const isValidNumber = !isNaN(numericValue) && value.trim() !== '';

  const effectiveMin = minimum_stake !== undefined ? minimum_stake : min;
  const effectiveMax = maximum_stake !== undefined ? maximum_stake : max;

  let hasError = false;
  let errorMessage = '';

  if (value !== '') {
    if (!isValidNumber) {
      hasError = true;
      errorMessage = 'Amount must be a number';
    } else if (minimum_stake !== undefined && numericValue < minimum_stake) {
      hasError = true;
      errorMessage = `Amount must be at least ${minimum_stake}`;
    } else if (effectiveMax !== undefined && numericValue > effectiveMax) {
      hasError = true;
      errorMessage = `Amount cannot exceed ${effectiveMax}`;
    } else if (minimum_stake === undefined && numericValue <= min) {
      hasError = true;
      errorMessage = `Amount must be greater than ${min}`;
    }
  }

  return (
    <div style={{ position: 'relative', display: 'inline-block', width: '100%' }}>
      <input
        id={id}
        type="text"
        inputMode="decimal"
        value={value}
        onChange={onChange}
        min={min}
        aria-describedby={hasError ? errorId : undefined}
        style={{
          width: '100%',
          paddingRight: `${tokenSymbol.length + 2}ch`, // extra space for suffix
          boxSizing: 'border-box',
        }}
      />
      <span
        style={{
          position: 'absolute',
          right: '8px',
          top: '50%',
          transform: 'translateY(-50%)',
          pointerEvents: 'none',
          color: '#555',
          fontWeight: 'bold',
        }}
        aria-hidden="true"
      >
        {tokenSymbol}
      </span>
      {hasError && (
        <span id={errorId} role="alert" style={{ color: 'red', fontSize: '0.875rem' }}>
          {errorMessage}
        </span>
      )}
    </div>
  );
};
