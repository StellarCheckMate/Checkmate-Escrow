import { vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { StakeAmountInput } from './StakeAmountInput';

describe('StakeAmountInput', () => {
  const onChange = vi.fn();

  test('renders token symbol suffix', () => {
    render(
      <StakeAmountInput tokenSymbol="ETH" value="" onChange={onChange} min={0} />
    );
    expect(screen.getByText('ETH')).toBeInTheDocument();
  });

  test('shows error for non‑numeric input', () => {
    render(
      <StakeAmountInput tokenSymbol="ETH" value="abc" onChange={onChange} min={0} />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Amount must be a number');
  });

  test('shows error for zero or negative input', () => {
    const { rerender } = render(
      <StakeAmountInput tokenSymbol="ETH" value="0" onChange={onChange} min={0} />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Amount must be greater than 0');

    rerender(
      <StakeAmountInput tokenSymbol="ETH" value="-5" onChange={onChange} min={0} />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Amount must be greater than 0');
  });

  test('calls onChange when user edits input', () => {
    render(
      <StakeAmountInput tokenSymbol="ETH" value="" onChange={onChange} min={0} />
    );
    const input = screen.getByRole('textbox');
    fireEvent.change(input, { target: { value: '10' } });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  test('blocks input below minimum_stake from contract config', () => {
    const { rerender } = render(
      <StakeAmountInput
        tokenSymbol="XLM"
        value="25"
        onChange={onChange}
        minimum_stake={50}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Amount must be at least 50');

    rerender(
      <StakeAmountInput
        tokenSymbol="XLM"
        value="50"
        onChange={onChange}
        minimum_stake={50}
      />
    );
    expect(screen.queryByRole('alert')).toBeNull();
  });

  test('blocks input exceeding maximum_stake from contract config', () => {
    const { rerender } = render(
      <StakeAmountInput
        tokenSymbol="XLM"
        value="1500"
        onChange={onChange}
        maximum_stake={1000}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Amount cannot exceed 1000');

    rerender(
      <StakeAmountInput
        tokenSymbol="XLM"
        value="1000"
        onChange={onChange}
        maximum_stake={1000}
      />
    );
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
