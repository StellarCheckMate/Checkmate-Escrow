import { useState } from 'react';
import { StakeAmountInput } from '../StakeAmountInput';
import { LichessGamePicker } from './LichessGamePicker';

export interface CreateMatchData {
  player2: string;
  stakeAmount: string;
  token: string;
  gameId: string;
  platform: 'lichess' | 'chessdotcom';
}

interface CreateMatchFormProps {
  onSubmit: (data: CreateMatchData) => void;
}

export const LICHESS_GAME_ID_REGEX = /^[a-zA-Z0-9]{8}$/;
export const CHESSDOTCOM_GAME_ID_REGEX = /^\d{7,12}$/;

export function validateGameId(gameId: string, platform: 'lichess' | 'chessdotcom'): string | null {
  const trimmed = gameId.trim();
  if (!trimmed) return 'Required';
  if (platform === 'lichess') {
    if (!LICHESS_GAME_ID_REGEX.test(trimmed)) {
      return 'Lichess game ID must be exactly 8 alphanumeric characters';
    }
  } else if (platform === 'chessdotcom') {
    if (!CHESSDOTCOM_GAME_ID_REGEX.test(trimmed)) {
      return 'Chess.com game ID must be 7-12 digits';
    }
  }
  return null;
}

export function CreateMatchForm({ onSubmit }: CreateMatchFormProps) {
  const [form, setForm] = useState<CreateMatchData>({
    player2: '',
    stakeAmount: '',
    token: '',
    gameId: '',
    platform: 'lichess',
  });
  const [errors, setErrors] = useState<Partial<Record<keyof CreateMatchData, string>>>({});

  function validate(): boolean {
    const e: typeof errors = {};
    if (!form.player2.trim()) e.player2 = 'Required';
    if (!form.stakeAmount.trim() || Number(form.stakeAmount) <= 0) e.stakeAmount = 'Must be > 0';
    if (!form.token.trim()) e.token = 'Required';
    const gameIdErr = validateGameId(form.gameId, form.platform);
    if (gameIdErr) e.gameId = gameIdErr;
    setErrors(e);
    return Object.keys(e).length === 0;
  }

  function handleSubmit(ev: React.FormEvent) {
    ev.preventDefault();
    if (validate()) onSubmit(form);
  }

  const set = (field: keyof CreateMatchData) =>
    (ev: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      setForm(f => ({ ...f, [field]: ev.target.value }));

  return (
    <form onSubmit={handleSubmit} noValidate>
      <div>
        <label htmlFor="player2">Player 2 Address</label>
        <input
          id="player2"
          value={form.player2}
          onChange={set('player2')}
          aria-label="Player 2 Address"
          aria-required="true"
          aria-invalid={errors.player2 ? 'true' : 'false'}
          aria-describedby={errors.player2 ? 'player2-error' : undefined}
        />
        {errors.player2 && (
          <span id="player2-error" role="alert">
            {errors.player2}
          </span>
        )}
      </div>
      <div>
        <label htmlFor="stakeAmount">Stake Amount</label>
        <StakeAmountInput
          id="stakeAmount"
          tokenSymbol={form.token || "TOKEN"}
          value={form.stakeAmount}
          onChange={set('stakeAmount')}
          min={0}
          aria-label="Stake Amount"
          aria-required="true"
          aria-invalid={errors.stakeAmount ? 'true' : 'false'}
          aria-describedby={errors.stakeAmount ? 'stakeAmount-error' : undefined}
        />
        {errors.stakeAmount && (
          <span id="stakeAmount-error" role="alert">
            {errors.stakeAmount}
          </span>
        )}
      </div>
      <div>
        <label htmlFor="token">Token Address</label>
        <input
          id="token"
          value={form.token}
          onChange={set('token')}
          aria-label="Token Address"
          aria-required="true"
          aria-invalid={errors.token ? 'true' : 'false'}
          aria-describedby={errors.token ? 'token-error' : undefined}
        />
        {errors.token && (
          <span id="token-error" role="alert">
            {errors.token}
          </span>
        )}
      </div>
      <div>
        <label htmlFor="platform">Platform</label>
        <select
          id="platform"
          value={form.platform}
          onChange={set('platform')}
          aria-label="Platform"
          aria-required="true"
        >
          <option value="lichess">Lichess</option>
          <option value="chessdotcom">Chess.com</option>
        </select>
      </div>
      <div>
        <label htmlFor="gameId">Game ID</label>
        <input
          id="gameId"
          value={form.gameId}
          onChange={set('gameId')}
          aria-label="Game ID"
          aria-required="true"
          aria-invalid={errors.gameId ? 'true' : 'false'}
          aria-describedby={errors.gameId ? 'gameId-error' : undefined}
        />
        {errors.gameId && (
          <span id="gameId-error" role="alert">
            {errors.gameId}
          </span>
        )}
      </div>
      {form.platform === 'lichess' && (
        <LichessGamePicker onSelect={gameId => setForm(f => ({ ...f, gameId }))} />
      )}
      <button type="submit">Create Match</button>
    </form>
  );
}
