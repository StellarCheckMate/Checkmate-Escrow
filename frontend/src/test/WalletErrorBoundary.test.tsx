import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { WalletErrorBoundary } from '../components/wallet/WalletErrorBoundary';

function Thrower({ message }: { message: string }): never {
  throw new Error(message);
}

describe('WalletErrorBoundary', () => {
  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  it('renders children when no error is thrown', () => {
    render(
      <WalletErrorBoundary>
        <span>safe content</span>
      </WalletErrorBoundary>
    );
    expect(screen.getByText('safe content')).toBeInTheDocument();
  });

  it('renders default fallback UI when a child throws', () => {
    render(
      <WalletErrorBoundary>
        <Thrower message="wallet exploded" />
      </WalletErrorBoundary>
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Wallet error: wallet exploded');
  });

  it('renders custom fallback when provided and a child throws', () => {
    render(
      <WalletErrorBoundary fallback={<p>custom fallback</p>}>
        <Thrower message="something went wrong" />
      </WalletErrorBoundary>
    );
    expect(screen.getByText('custom fallback')).toBeInTheDocument();
  });
});
