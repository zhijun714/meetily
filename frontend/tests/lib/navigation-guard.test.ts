import { describe, expect, test } from 'bun:test';
import { registerNavigationGuard, requestNavigation } from '../../src/lib/navigation-guard';

describe('template navigation protection', () => {
  test('cancel preserves editor and unregister restores normal navigation', async () => {
    let count = 0;
    const unregister = registerNavigationGuard(async () => false);
    try {
      await requestNavigation(() => count++);
      expect(count).toBe(0);
    } finally { unregister(); }
    await requestNavigation(() => count++);
    expect(count).toBe(1);
  });

  test('concurrent navigation waits for one decision and stale cleanup preserves new guard', async () => {
    let resolve!: (allow: boolean) => void;
    let count = 0;
    const old = registerNavigationGuard(async () => true);
    const current = registerNavigationGuard(() => new Promise<boolean>(done => { resolve = done; }));
    old();
    try {
      const first = requestNavigation(() => count++);
      await requestNavigation(() => count += 10);
      expect(count).toBe(0);
      resolve(true);
      await first;
      expect(count).toBe(1);
    } finally { current(); }
  });
});
