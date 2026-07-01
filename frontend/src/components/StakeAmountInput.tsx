import React, { useId } from 'react';

type StakeAmountInputProps = {
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
};

/**
 * Input component for entering a staking amount.
 * Displays the token symbol as a suffix and performs inline validation.
 * Uses a plain text input (type="text") so it works with the test suite's
 * `getByRole('textbox')` query while still allowing numeric validation.
 */
export const StakeAmountInput: React.FC<StakeAmountInputProps> = ({
  id,
  tokenSymbol,
  value,
  onChange,
  min = 0,
}) => {
  const errorId = useId();
  const numericValue = Number(value);
  const isValidNumber = !isNaN(numericValue) && value.trim() !== '';
  const hasError = value !== '' && (!isValidNumber || numericValue <= min);
  const errorMessage =
    !isValidNumber
      ? 'Amount must be a number'
      : numericValue <= min
      ? `Amount must be greater than ${min}`
      : '';

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
