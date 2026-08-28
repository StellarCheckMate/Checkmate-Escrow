import { useState } from 'react';
import type { WalletState } from '../wallets/types';

export interface WaveIssue {
  id: number;
  title: string;
  url: string;
  difficulty: 'easy' | 'medium' | 'hard';
  points: number;
  labels: string[];
  comments: number;
  state: 'open' | 'closed';
}

export interface ContributorLeaderboardEntry {
  rank: number;
  username: string;
  points: number;
  completedIssues: number;
  avatarUrl?: string;
}

const MOCK_LEADERBOARD: ContributorLeaderboardEntry[] = [
  { rank: 1, username: 's6pa1rta3n-lab', points: 1450, completedIssues: 12 },
  { rank: 2, username: 'stellar-builder-pro', points: 920, completedIssues: 7 },
  { rank: 3, username: 'soroban-dev-99', points: 650, completedIssues: 5 },
  { rank: 4, username: 'crypto-sage', points: 400, completedIssues: 3 },
  { rank: 5, username: 'chess-web3-coder', points: 250, completedIssues: 2 },
];

const DEFAULT_WAVE_ISSUES: WaveIssue[] = [
  {
    id: 1435,
    title: 'Enhancement: add contribution leaderboard page to the frontend for Drips Wave participants',
    url: 'https://github.com/StellarCheckMate/Checkmate-Escrow/issues/1435',
    difficulty: 'medium',
    points: 150,
    labels: ['enhancement', 'community', 'wave-ready'],
    comments: 1,
    state: 'open',
  },
  {
    id: 1436,
    title: 'Good First Issue: Update documentation for Soroban RPC endpoints',
    url: 'https://github.com/StellarCheckMate/Checkmate-Escrow/issues/1436',
    difficulty: 'easy',
    points: 50,
    labels: ['good first issue', 'docs', 'wave-ready'],
    comments: 0,
    state: 'open',
  },
  {
    id: 1437,
    title: 'Security: Implement Soroban Contract State Proof Merkle Tree Verification Service',
    url: 'https://github.com/StellarCheckMate/Checkmate-Escrow/issues/1437',
    difficulty: 'hard',
    points: 250,
    labels: ['security', 'soroban', 'wave-ready'],
    comments: 2,
    state: 'open',
  },
];

interface Props {
  wallet?: WalletState;
  initialIssues?: WaveIssue[];
  initialLeaderboard?: ContributorLeaderboardEntry[];
}

export function ContributePage({
  wallet,
  initialIssues = DEFAULT_WAVE_ISSUES,
  initialLeaderboard = MOCK_LEADERBOARD,
}: Props) {
  const [issues] = useState<WaveIssue[]>(initialIssues);
  const [leaderboard] = useState<ContributorLeaderboardEntry[]>(initialLeaderboard);
  const [selectedDifficulty, setSelectedDifficulty] = useState<'all' | 'easy' | 'medium' | 'hard'>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const filteredIssues = issues.filter((issue) => {
    const matchesDifficulty = selectedDifficulty === 'all' || issue.difficulty === selectedDifficulty;
    const matchesSearch =
      issue.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
      issue.labels.some((l) => l.toLowerCase().includes(searchQuery.toLowerCase()));
    return matchesDifficulty && matchesSearch;
  });

  const easyIssues = filteredIssues.filter((i) => i.difficulty === 'easy');
  const mediumIssues = filteredIssues.filter((i) => i.difficulty === 'medium');
  const hardIssues = filteredIssues.filter((i) => i.difficulty === 'hard');

  return (
    <main id="contribute-page" className="contribute-container" aria-label="Drips Wave Contribution Hub">
      <header className="contribute-header">
        <nav aria-label="Breadcrumb">
          <a href="/" className="nav-back-link">
            ← Back to App
          </a>
        </nav>
        <h1>🌊 Drips Wave Contributor Hub</h1>
        <p className="subtitle">
          Contribute to Checkmate-Escrow, earn Wave points, and claim a share of the community reward pool.
        </p>
        {wallet?.connected && wallet.publicKey && (
          <div className="connected-badge" role="status">
            Connected as: <span className="mono">{wallet.publicKey.slice(0, 8)}...{wallet.publicKey.slice(-6)}</span>
          </div>
        )}
      </header>

      <section className="leaderboard-section" aria-labelledby="leaderboard-heading">
        <h2 id="leaderboard-heading">🏆 Contributor Points Leaderboard</h2>
        <div className="leaderboard-table-wrapper" tabIndex={0} role="region" aria-label="Contributor Points Table">
          <table className="leaderboard-table">
            <thead>
              <tr>
                <th scope="col">Rank</th>
                <th scope="col">Contributor</th>
                <th scope="col">Completed Issues</th>
                <th scope="col">Wave Points</th>
              </tr>
            </thead>
            <tbody>
              {leaderboard.map((entry) => (
                <tr key={entry.rank} className={entry.rank === 1 ? 'top-rank' : ''}>
                  <td className="rank-cell">#{entry.rank}</td>
                  <td className="contributor-cell">
                    <strong>{entry.username}</strong>
                  </td>
                  <td>{entry.completedIssues}</td>
                  <td>
                    <span className="points-badge">{entry.points} pts</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="issues-section" aria-labelledby="issues-heading">
        <div className="issues-header-bar">
          <h2 id="issues-heading">📌 Wave-Ready Issues</h2>
          <div className="controls-group">
            <label htmlFor="issue-search" className="sr-only">
              Search issues
            </label>
            <input
              id="issue-search"
              type="search"
              placeholder="Search issues or tags..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="search-input"
              aria-label="Search issues or tags"
            />

            <div className="filter-buttons" role="group" aria-label="Filter issues by difficulty">
              {(['all', 'easy', 'medium', 'hard'] as const).map((diff) => (
                <button
                  key={diff}
                  type="button"
                  className={`filter-btn ${selectedDifficulty === diff ? 'active' : ''}`}
                  onClick={() => setSelectedDifficulty(diff)}
                  aria-pressed={selectedDifficulty === diff}
                >
                  {diff.toUpperCase()}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="issues-grid">
          {(selectedDifficulty === 'all' || selectedDifficulty === 'easy') && easyIssues.length > 0 && (
            <div className="difficulty-group" aria-labelledby="easy-heading">
              <h3 id="easy-heading" className="difficulty-title easy">
                🟢 Good First Issues ({easyIssues.length})
              </h3>
              <ul className="issue-card-list">
                {easyIssues.map((issue) => (
                  <li key={issue.id} className="issue-card">
                    <div className="issue-header">
                      <span className="issue-number">#{issue.id}</span>
                      <span className="points-tag">{issue.points} pts</span>
                    </div>
                    <h4>
                      <a href={issue.url} target="_blank" rel="noopener noreferrer">
                        {issue.title}
                      </a>
                    </h4>
                    <div className="tags-list">
                      {issue.labels.map((l) => (
                        <span key={l} className="label-pill">
                          {l}
                        </span>
                      ))}
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {(selectedDifficulty === 'all' || selectedDifficulty === 'medium') && mediumIssues.length > 0 && (
            <div className="difficulty-group" aria-labelledby="medium-heading">
              <h3 id="medium-heading" className="difficulty-title medium">
                🟡 Medium Priority ({mediumIssues.length})
              </h3>
              <ul className="issue-card-list">
                {mediumIssues.map((issue) => (
                  <li key={issue.id} className="issue-card">
                    <div className="issue-header">
                      <span className="issue-number">#{issue.id}</span>
                      <span className="points-tag">{issue.points} pts</span>
                    </div>
                    <h4>
                      <a href={issue.url} target="_blank" rel="noopener noreferrer">
                        {issue.title}
                      </a>
                    </h4>
                    <div className="tags-list">
                      {issue.labels.map((l) => (
                        <span key={l} className="label-pill">
                          {l}
                        </span>
                      ))}
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {(selectedDifficulty === 'all' || selectedDifficulty === 'hard') && hardIssues.length > 0 && (
            <div className="difficulty-group" aria-labelledby="hard-heading">
              <h3 id="hard-heading" className="difficulty-title hard">
                🔴 Advanced / Soroban ({hardIssues.length})
              </h3>
              <ul className="issue-card-list">
                {hardIssues.map((issue) => (
                  <li key={issue.id} className="issue-card">
                    <div className="issue-header">
                      <span className="issue-number">#{issue.id}</span>
                      <span className="points-tag">{issue.points} pts</span>
                    </div>
                    <h4>
                      <a href={issue.url} target="_blank" rel="noopener noreferrer">
                        {issue.title}
                      </a>
                    </h4>
                    <div className="tags-list">
                      {issue.labels.map((l) => (
                        <span key={l} className="label-pill">
                          {l}
                        </span>
                      ))}
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {filteredIssues.length === 0 && (
            <div className="no-issues-state" role="status">
              <p>No wave-ready issues matching "{searchQuery}".</p>
            </div>
          )}
        </div>
      </section>
    </main>
  );
}
