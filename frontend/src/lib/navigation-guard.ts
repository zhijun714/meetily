type NavigationGuard = () => Promise<boolean>;
let currentGuard: NavigationGuard | null = null;
let pending = false;

export function registerNavigationGuard(guard: NavigationGuard): () => void {
  currentGuard = guard;
  return () => { if (currentGuard === guard) currentGuard = null; };
}

export async function requestNavigation(action: () => void): Promise<void> {
  if (pending) return;
  pending = true;
  try {
    if (!currentGuard || await currentGuard()) action();
  } finally {
    pending = false;
  }
}
