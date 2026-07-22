import { createSignal } from 'solid-js';
import type { Accessor } from 'solid-js';

export type SidebarWipeDirection = 'expand' | 'collapse';

const WIPE_DURATION_MS = 200;
const WIPE_CLEANUP_BUFFER_MS = 40;

const [wipe, setWipe] = createSignal<SidebarWipeDirection | null>(null);
let cleanupTimer: ReturnType<typeof setTimeout> | null = null;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export interface SidebarWipe {
  wipe: Accessor<SidebarWipeDirection | null>;
}

/**
 * Masks the sidebar's instant layout snap with a compositor-only wipe.
 * Call with the NEW collapsed state right when the preference flips; the
 * sidebar and main content read the shared signal to run their transforms.
 */
export function startSidebarWipe(nextCollapsed: boolean): void {
  if (cleanupTimer) {
    clearTimeout(cleanupTimer);
    cleanupTimer = null;
  }
  if (prefersReducedMotion()) {
    setWipe(null);
    return;
  }
  setWipe(nextCollapsed ? 'collapse' : 'expand');
  cleanupTimer = setTimeout(() => {
    cleanupTimer = null;
    setWipe(null);
  }, WIPE_DURATION_MS + WIPE_CLEANUP_BUFFER_MS);
}

export function createSidebarWipe(): SidebarWipe {
  return { wipe };
}

export function resetSidebarWipe(): void {
  if (cleanupTimer) {
    clearTimeout(cleanupTimer);
    cleanupTimer = null;
  }
  setWipe(null);
}
