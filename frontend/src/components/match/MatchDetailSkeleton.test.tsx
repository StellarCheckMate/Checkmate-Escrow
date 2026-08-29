import { render } from '@testing-library/react';
import { MatchDetailSkeleton } from './MatchDetailSkeleton';

describe('MatchDetailSkeleton', () => {
  test('matches snapshot', () => {
    const { container } = render(<MatchDetailSkeleton />);
    expect(container).toMatchSnapshot();
  });

  test('exposes an accessible loading status', () => {
    const { getByRole } = render(<MatchDetailSkeleton />);
    expect(getByRole('status')).toHaveAttribute('aria-label', 'Loading match details');
  });
});
