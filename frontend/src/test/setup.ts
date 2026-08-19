import '@testing-library/jest-dom';
import { vi } from 'vitest';

// @testing-library/dom's `waitFor` only auto-detects fake timers via a
// `jest` global (it doesn't know about Vitest's `vi`), and it drives its
// internal polling loop by calling `jest.advanceTimersByTime` directly.
// Without this shim, `waitFor` silently polls with real timers while
// `vi.useFakeTimers()` is active, so it never observes state changes and
// hangs until it times out. See https://github.com/testing-library/dom-testing-library/issues/830
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).jest = {
  advanceTimersByTime: (ms: number) => vi.advanceTimersByTime(ms),
};
