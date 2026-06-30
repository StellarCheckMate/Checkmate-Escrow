import { useEffect, useRef, useState } from 'react';

interface Props {
  title: string;
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
  countdownSeconds?: number;
}

export function ConfirmDialog({ title, message, onConfirm, onCancel, countdownSeconds = 10 }: Props) {
  const [remaining, setRemaining] = useState(countdownSeconds);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    intervalRef.current = setInterval(() => {
      setRemaining(n => {
        if (n <= 1) {
          clearInterval(intervalRef.current!);
          return 0;
        }
        return n - 1;
      });
    }, 1000);
    return () => clearInterval(intervalRef.current!);
  }, []);

  return (
    <div role="dialog" aria-modal="true" aria-labelledby="confirm-title">
      <h2 id="confirm-title">{title}</h2>
      <p>{message}</p>
      <p aria-live="polite">
        {remaining > 0
          ? `Confirm available in ${remaining}s…`
          : 'Ready to confirm.'}
      </p>
      <div>
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          onClick={onConfirm}
          disabled={remaining > 0}
          aria-disabled={remaining > 0}
        >
          Confirm
        </button>
      </div>
    </div>
  );
}
