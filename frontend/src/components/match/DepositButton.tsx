import { useState } from 'react';

interface DepositButtonProps {
  matchId: number;
  disabled?: boolean;
  onDeposit: (matchId: number) => Promise<void>;
}

export function DepositButton({ matchId, disabled, onDeposit }: DepositButtonProps) {
  const [loading, setLoading] = useState(false);
  const [feedback, setFeedback] = useState<{ ok: boolean; msg: string } | null>(null);

  async function handleClick() {
    setLoading(true);
    setFeedback(null);
    try {
      await onDeposit(matchId);
      setFeedback({ ok: true, msg: 'Deposit successful!' });
    } catch (e) {
      setFeedback({ ok: false, msg: e instanceof Error ? e.message : 'Deposit failed.' });
    } finally {
      setLoading(false);
    }
  }

  return (
    <div>
      <button onClick={handleClick} disabled={disabled || loading} aria-busy={loading}>
        {loading ? <span aria-label="loading">⏳</span> : 'Deposit'}
      </button>
      {feedback && (
        <p role="status" style={{ color: feedback.ok ? 'green' : 'red' }}>
          {feedback.msg}
        </p>
      )}
    </div>
  );
}
