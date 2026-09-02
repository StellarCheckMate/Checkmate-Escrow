import { useWallet } from './hooks/useWallet'
import { WalletConnector } from './components/wallet/WalletConnector'
import { AdminPanel } from './pages/AdminPanel'
import { MatchDetailPage } from './pages/MatchDetailPage'
import './App.css'

/** Matches deep-links of the form /match/1234 */
const MATCH_ROUTE = /^\/match\/(\d+)$/

function App() {
  const wallet = useWallet()
  const isAdmin = new URLSearchParams(window.location.search).get('admin') === '1'
    || window.location.pathname === '/admin'

  const matchRouteMatch = window.location.pathname.match(MATCH_ROUTE)

  if (isAdmin) {
    return <AdminPanel wallet={wallet} />
  }

  if (matchRouteMatch) {
    return <MatchDetailPage matchId={Number(matchRouteMatch[1])} />
  }

  return (
    <main id="landing">
      <h1>Checkmate-Escrow</h1>
      <p className="tagline">Trustless chess wagering on Stellar — stake, play, get paid instantly.</p>
      <WalletConnector wallet={wallet} />
    </main>
  )
}

export default App
