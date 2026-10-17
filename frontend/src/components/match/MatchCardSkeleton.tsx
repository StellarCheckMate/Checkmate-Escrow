export function MatchCardSkeleton() {
  return (
    <div
      className="match-card match-card--skeleton"
      aria-hidden="true"
    >
      <div className="match-card__header">
        <span className="skeleton-block skeleton-block--id" />
        <span className="skeleton-block skeleton-block--badge" />
      </div>
      <dl className="match-card__details">
        <div className="match-card__row">
          <dt>Player 1</dt>
          <dd>
            <span className="skeleton-block skeleton-block--value" />
          </dd>
        </div>
        <div className="match-card__row">
          <dt>Player 2</dt>
          <dd>
            <span className="skeleton-block skeleton-block--value" />
          </dd>
        </div>
        <div className="match-card__row">
          <dt>Stake</dt>
          <dd>
            <span className="skeleton-block skeleton-block--value" />
          </dd>
        </div>
        <div className="match-card__row">
          <dt>Platform</dt>
          <dd>
            <span className="skeleton-block skeleton-block--value" />
          </dd>
        </div>
      </dl>
    </div>
  );
}

export default MatchCardSkeleton;