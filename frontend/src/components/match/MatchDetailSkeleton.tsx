export function MatchDetailSkeleton() {
  return (
    <div className="match-detail-skeleton" role="status" aria-live="polite" aria-label="Loading match details">
      <div className="skeleton-line skeleton-title" />
      <div className="skeleton-row">
        <div className="skeleton-avatar" />
        <div className="skeleton-line skeleton-short" />
        <span className="skeleton-vs">vs</span>
        <div className="skeleton-avatar" />
        <div className="skeleton-line skeleton-short" />
      </div>
      <div className="skeleton-line skeleton-medium" />
      <div className="skeleton-line skeleton-medium" />
      <div className="skeleton-line skeleton-short" />
      <span className="sr-only">Loading match details…</span>
    </div>
  );
}
