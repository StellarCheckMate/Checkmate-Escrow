# Checkmate Escrow Frontend

Frontend utilities and hooks for interacting with the Checkmate Escrow smart contract on Stellar.

## Hooks

### `useMatchStatus`

A React hook that polls for match state changes from the Soroban escrow contract.

#### Features

- 🔄 Automatic polling at configurable intervals (default: 10 seconds)
- 🛑 Smart polling that stops when match reaches terminal states (Completed/Cancelled)
- 🧹 Automatic cleanup on unmount
- ⚡ Manual refetch capability
- 🎯 TypeScript support with full type definitions
- ✅ Comprehensive test coverage

#### Usage

```typescript
import { useMatchStatus } from './hooks/useMatchStatus';
import { SorobanClient } from '@stellar/stellar-sdk';

function MatchMonitor({ matchId }: { matchId: number }) {
  const { match, loading, error, refetch } = useMatchStatus(
    matchId,
    async (id) => {
      // Your contract call implementation
      const contract = new SorobanClient.Contract(contractId);
      const result = await contract.call('get_match', { match_id: id });
      return result;
    },
    {
      interval: 5000, // Poll every 5 seconds (optional)
      enabled: true,  // Enable polling (optional)
    }
  );

  if (loading) return <div>Loading match data...</div>;
  if (error) return <div>Error: {error.message}</div>;
  if (!match) return <div>Match not found</div>;

  return (
    <div>
      <h2>Match {match.id}</h2>
      <p>State: {match.state}</p>
      <p>Player 1: {match.player1}</p>
      <p>Player 2: {match.player2}</p>
      <p>Stake: {match.stake_amount}</p>
      <button onClick={refetch}>Refresh Now</button>
    </div>
  );
}
```

#### API

##### Parameters

- `matchId: number` - The ID of the match to monitor
- `getMatchFn: (matchId: number) => Promise<Match>` - Function that fetches match data from the contract
- `options?: UseMatchStatusOptions` - Configuration options:
  - `interval?: number` - Polling interval in milliseconds (default: 10000)
  - `enabled?: boolean` - Whether to enable polling (default: true)

##### Return Value

Returns a `UseMatchStatusReturn` object:

```typescript
{
  match: Match | null;        // Current match data
  loading: boolean;           // Loading state
  error: Error | null;        // Error state
  refetch: () => Promise<void>; // Manual refetch function
}
```

##### Types

```typescript
enum MatchState {
  Pending = 'Pending',
  Active = 'Active',
  Completed = 'Completed',
  Cancelled = 'Cancelled',
}

interface Match {
  id: number;
  player1: string;
  player2: string;
  stake_amount: string;
  token: string;
  game_id: string;
  platform: Platform;
  state: MatchState;
  player1_deposited: boolean;
  player2_deposited: boolean;
  created_ledger: number;
}
```

#### Behavior

1. **Initial Fetch**: Fetches match data immediately on mount
2. **Polling**: Continues polling at the specified interval while match is in `Pending` or `Active` state
3. **Auto-Stop**: Stops polling when match reaches `Completed` or `Cancelled` state
4. **Cleanup**: Clears all intervals on unmount
5. **Re-fetch**: Restarts polling when `matchId` changes
6. **Manual Refresh**: Exposes `refetch()` function for manual updates

#### Testing

Run the test suite:

```bash
npm test
```

Run tests in watch mode:

```bash
npm run test:watch
```

Generate coverage report:

```bash
npm run test:coverage
```

The hook includes comprehensive tests covering:
- Initial fetch behavior
- Polling intervals
- Terminal state detection
- Cleanup on unmount
- Match ID changes
- Enable/disable toggling
- Manual refetch
- Error handling

## Development

### Setup

```bash
cd frontend
npm install
```

### Type Checking

```bash
npx tsc --noEmit
```

## License

MIT
