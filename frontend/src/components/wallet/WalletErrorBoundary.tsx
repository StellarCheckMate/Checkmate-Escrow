import { Component, type ReactNode } from 'react';
import { FreighterNotInstalledError } from '../../wallets/freighter';

interface Props { children: ReactNode; fallback?: ReactNode; }
interface State { error: Error | null; }

export class WalletErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      if (this.props.fallback) return this.props.fallback;

      const isFreighterNotInstalled =
        this.state.error instanceof FreighterNotInstalledError ||
        this.state.error.name === 'FreighterNotInstalledError' ||
        this.state.error.message.toLowerCase().includes('freighter not installed') ||
        this.state.error.message.toLowerCase().includes('freighter wallet not detected') ||
        this.state.error.message.toLowerCase().includes('freighter is not installed');

      if (isFreighterNotInstalled) {
        return (
          <div role="alert">
            <p>
              Freighter wallet not detected.{' '}
              <a
                href="https://www.freighter.app/"
                target="_blank"
                rel="noopener noreferrer"
              >
                Install Freighter
              </a>
            </p>
          </div>
        );
      }

      return (
        <p role="alert">Wallet error: {this.state.error.message}</p>
      );
    }
    return this.props.children;
  }
}
