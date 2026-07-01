import { useState } from 'react';
import { StakeAmountInput } from '../StakeAmountInput';

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
    if (!form.gameId.trim()) e.gameId = 'Required';
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
        <input id="player2" value={form.player2} onChange={set('player2')} />
        {errors.player2 && <span role="alert">{errors.player2}</span>}
      </div>
      <div>
        <label htmlFor="stakeAmount">Stake Amount</label>
        <StakeAmountInput
          id="stakeAmount"
          tokenSymbol={form.token || "TOKEN"}
          value={form.stakeAmount}
          onChange={set('stakeAmount')}
          min={0}
        />
      </div>
      <div>
        <label htmlFor="token">Token Address</label>
        <input id="token" value={form.token} onChange={set('token')} />
        {errors.token && <span role="alert">{errors.token}</span>}
      </div>
      <div>
        <label htmlFor="gameId">Game ID</label>
        <input id="gameId" value={form.gameId} onChange={set('gameId')} />
        {errors.gameId && <span role="alert">{errors.gameId}</span>}
      </div>
      <div>
        <label htmlFor="platform">Platform</label>
        <select id="platform" value={form.platform} onChange={set('platform')}>
          <option value="lichess">Lichess</option>
          <option value="chessdotcom">Chess.com</option>
        </select>
      </div>
      <button type="submit">Create Match</button>
    </form>
  );
}
