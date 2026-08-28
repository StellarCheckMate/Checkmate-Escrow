import { useWallet } from './hooks/useWallet'
import { WalletConnector } from './components/wallet/WalletConnector'
import { AdminPanel } from './pages/AdminPanel'
import { ContributePage } from './pages/ContributePage'
import './App.css'

function App() {
  const wallet = useWallet()
  const searchParams = new URLSearchParams(window.location.search)
  const isAdmin = searchParams.get('admin') === '1' || window.location.pathname === '/admin'
  const isContribute = searchParams.get('page') === 'contribute' || window.location.pathname === '/contribute'

  if (isAdmin) {
    return <AdminPanel wallet={wallet} />
  }

  if (isContribute) {
    return <ContributePage wallet={wallet} />
  }

  return (
    <main id="landing">
      <h1>Checkmate-Escrow</h1>
      <p className="tagline">Trustless chess wagering on Stellar — stake, play, get paid instantly.</p>
      <nav className="landing-nav" aria-label="Main Navigation">
        <a href="/contribute" className="nav-contribute-link">
          🌊 Drips Wave Leaderboard & Issues →
        </a>
      </nav>
      <WalletConnector wallet={wallet} />
    </main>
  )
}

export default App
