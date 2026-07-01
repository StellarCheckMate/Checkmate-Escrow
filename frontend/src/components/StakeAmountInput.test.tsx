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
});
