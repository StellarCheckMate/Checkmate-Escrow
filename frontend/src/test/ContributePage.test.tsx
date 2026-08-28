import { render, screen, fireEvent } from '@testing-library/react'
import { describe, it, expect } from 'vitest'
import { ContributePage } from '../pages/ContributePage'
import type { WaveIssue, ContributorLeaderboardEntry } from '../pages/ContributePage'

const customIssues: WaveIssue[] = [
  {
    id: 101,
    title: 'Easy issue: fix typo in README',
    url: 'https://github.com/example/101',
    difficulty: 'easy',
    points: 50,
    labels: ['good first issue', 'docs'],
    comments: 0,
    state: 'open',
  },
  {
    id: 102,
    title: 'Medium issue: add leaderboard page',
    url: 'https://github.com/example/102',
    difficulty: 'medium',
    points: 150,
    labels: ['enhancement', 'wave-ready'],
    comments: 2,
    state: 'open',
  },
  {
    id: 103,
    title: 'Hard issue: Soroban Merkle verifier',
    url: 'https://github.com/example/103',
    difficulty: 'hard',
    points: 250,
    labels: ['security', 'soroban'],
    comments: 4,
    state: 'open',
  },
]

const customLeaderboard: ContributorLeaderboardEntry[] = [
  { rank: 1, username: 's6pa1rta3n-lab', points: 1500, completedIssues: 10 },
  { rank: 2, username: 'dev-runner', points: 800, completedIssues: 5 },
]

describe('ContributePage', () => {
  it('renders heading, leaderboard, and issues section with accessible landmarks', () => {
    render(
      <ContributePage
        initialIssues={customIssues}
        initialLeaderboard={customLeaderboard}
      />
    )

    expect(screen.getByRole('main', { name: /Drips Wave Contribution Hub/i })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1, name: /Drips Wave Contributor Hub/i })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 2, name: /Contributor Points Leaderboard/i })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 2, name: /Wave-Ready Issues/i })).toBeInTheDocument()
  })

  it('renders leaderboard entries with proper ranks and points badges', () => {
    render(
      <ContributePage
        initialIssues={customIssues}
        initialLeaderboard={customLeaderboard}
      />
    )

    expect(screen.getByText('s6pa1rta3n-lab')).toBeInTheDocument()
    expect(screen.getByText('1500 pts')).toBeInTheDocument()
    expect(screen.getByText('dev-runner')).toBeInTheDocument()
    expect(screen.getByText('800 pts')).toBeInTheDocument()
  })

  it('groups wave issues by difficulty levels (easy, medium, hard)', () => {
    render(
      <ContributePage
        initialIssues={customIssues}
        initialLeaderboard={customLeaderboard}
      />
    )

    expect(screen.getByText(/Good First Issues \(1\)/i)).toBeInTheDocument()
    expect(screen.getByText(/Medium Priority \(1\)/i)).toBeInTheDocument()
    expect(screen.getByText(/Advanced \/ Soroban \(1\)/i)).toBeInTheDocument()
    expect(screen.getByText('Easy issue: fix typo in README')).toBeInTheDocument()
    expect(screen.getByText('Medium issue: add leaderboard page')).toBeInTheDocument()
    expect(screen.getByText('Hard issue: Soroban Merkle verifier')).toBeInTheDocument()
  })

  it('filters issues by difficulty filter buttons', () => {
    render(
      <ContributePage
        initialIssues={customIssues}
        initialLeaderboard={customLeaderboard}
      />
    )

    const easyFilter = screen.getByRole('button', { name: 'EASY' })
    fireEvent.click(easyFilter)

    expect(screen.getByText('Easy issue: fix typo in README')).toBeInTheDocument()
    expect(screen.queryByText('Medium issue: add leaderboard page')).not.toBeInTheDocument()
    expect(screen.queryByText('Hard issue: Soroban Merkle verifier')).not.toBeInTheDocument()

    const mediumFilter = screen.getByRole('button', { name: 'MEDIUM' })
    fireEvent.click(mediumFilter)

    expect(screen.queryByText('Easy issue: fix typo in README')).not.toBeInTheDocument()
    expect(screen.getByText('Medium issue: add leaderboard page')).toBeInTheDocument()
  })

  it('filters issues by search query across titles and tags', () => {
    render(
      <ContributePage
        initialIssues={customIssues}
        initialLeaderboard={customLeaderboard}
      />
    )

    const searchInput = screen.getByRole('searchbox', { name: /search issues/i })
    fireEvent.change(searchInput, { target: { value: 'Soroban' } })

    expect(screen.getByText('Hard issue: Soroban Merkle verifier')).toBeInTheDocument()
    expect(screen.queryByText('Easy issue: fix typo in README')).not.toBeInTheDocument()

    fireEvent.change(searchInput, { target: { value: 'nonexistent-string-xyz' } })
    expect(screen.getByText(/No wave-ready issues matching/i)).toBeInTheDocument()
  })

  it('displays connected wallet badge when wallet is connected', () => {
    render(
      <ContributePage
        wallet={{
          connected: true,
          publicKey: 'GCL6OXAMLD75BMTINA6EMRUDWK5THQUSHMYNLSNBCJAPZJHNYJTUNIBC',
          type: 'freighter',
          error: null,
        }}
        initialIssues={customIssues}
        initialLeaderboard={customLeaderboard}
      />
    )

    expect(screen.getByRole('status')).toHaveTextContent(/Connected as: GCL6OXAM...TUNIBC/i)
  })
})
