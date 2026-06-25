import { useWallet } from './hooks/useWallet'
import { WalletConnector } from './components/wallet/WalletConnector'
import './App.css'

function App() {
  const wallet = useWallet()

  return (
    <main id="landing">
      <h1>Checkmate-Escrow</h1>
      <p className="tagline">Trustless chess wagering on Stellar — stake, play, get paid instantly.</p>
      <WalletConnector wallet={wallet} />
    </main>
  )
}

export default App
