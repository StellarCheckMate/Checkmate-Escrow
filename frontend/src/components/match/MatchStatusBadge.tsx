import type { CSSProperties } from 'react';

type MatchStatus = 'pending' | 'active' | 'completed' | 'cancelled';

const bg: Record<MatchStatus, string> = {
  pending: '#ca8a04',
  active: '#2563eb',
  completed: '#16a34a',
  cancelled: '#6b7280',
};

const style: CSSProperties = {
  color: '#fff',
  borderRadius: '9999px',
  padding: '2px 10px',
  fontSize: '0.75rem',
  fontWeight: 600,
  display: 'inline-block',
};

export function MatchStatusBadge({ status }: { status: MatchStatus }) {
  return (
    <span aria-label={status} style={{ ...style, backgroundColor: bg[status] }}>
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}
