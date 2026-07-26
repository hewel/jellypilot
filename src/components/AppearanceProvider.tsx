import type { Appearance } from '@bindings';
import { Effect, Exit } from 'effect';
import {
  createContext,
  createSignal,
  onCleanup,
  useContext,
  type Accessor,
  type ParentProps,
} from 'solid-js';
import {
  applyAppearanceRootAttributes,
  appearancesEqual,
  persistAppearanceSelection,
  resolveComputedCanvas,
  type BootstrappedAppearance,
} from '~effects/appearance';
import { commandFailureMessage } from '~effects/commands';

import { useToast } from './ToastProvider';

export interface AppearanceContextValue {
  readonly desired: Accessor<Appearance>;
  readonly confirmed: Accessor<Appearance>;
  readonly saving: Accessor<boolean>;
  readonly selectAppearance: (appearance: Appearance) => void;
  readonly selectDesignTheme: (designTheme: Appearance['designTheme']) => void;
  readonly selectColorMode: (colorMode: Appearance['colorMode']) => void;
}

const AppearanceContext = createContext<AppearanceContextValue>();

function readRootCanvas(): BootstrappedAppearance['canvas'] | null {
  return (
    resolveComputedCanvas(document.body) ?? resolveComputedCanvas(document.documentElement) ?? null
  );
}

function waitForPaint(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  requestAnimationFrame(() => resolve());
  return promise;
}

export function AppearanceProvider(
  props: ParentProps<{ readonly initial: BootstrappedAppearance }>,
) {
  const { showToast } = useToast();
  const [desired, setDesired] = createSignal(props.initial.appearance);
  const [confirmed, setConfirmed] = createSignal(props.initial.appearance);
  const [saving, setSaving] = createSignal(false);

  let writeInFlight = false;
  let queued: Appearance | null = null;
  let generation = 0;
  let disposed = false;

  onCleanup(() => {
    disposed = true;
  });

  const applyRoot = (appearance: Appearance) => {
    applyAppearanceRootAttributes(document.documentElement, appearance);
  };

  const runWriteLoop = async () => {
    if (writeInFlight || disposed) return;
    writeInFlight = true;
    setSaving(true);

    try {
      for (;;) {
        if (disposed) return;

        // Always coalesce to the newest selection before capturing canvas / IPC.
        const next = queued ?? desired();
        queued = null;

        if (appearancesEqual(next, confirmed())) {
          if (queued) continue;
          break;
        }

        const writeGeneration = ++generation;
        applyRoot(next);
        setDesired(next);

        await waitForPaint();
        if (disposed) return;

        // A newer selection may have replaced the root during the paint wait.
        // Never pair the older Appearance with the newer root's canvas.
        if (
          writeGeneration !== generation ||
          queued !== null ||
          !appearancesEqual(next, desired())
        ) {
          continue;
        }

        const canvas = readRootCanvas();
        if (!canvas) {
          if (writeGeneration !== generation || queued || !appearancesEqual(next, desired())) {
            continue;
          }
          applyRoot(confirmed());
          setDesired(confirmed());
          showToast('error', 'Could not resolve appearance canvas');
          continue;
        }

        // Capture the exact pair for this request only after paint coalescing.
        const request = { appearance: next, canvas } as const;

        const exit = await Effect.runPromiseExit(persistAppearanceSelection(request));

        const isNewest =
          writeGeneration === generation && queued === null && appearancesEqual(next, desired());

        if (Exit.isSuccess(exit)) {
          // Writes are serialized, so every success advances persisted/native
          // truth even when a newer optimistic selection is already queued.
          // Advance confirmed without rolling back the newer root; the loop then
          // drains the queue, so a queued return to the prior selection still
          // issues its compensating write instead of being skipped as a no-op.
          setConfirmed(next);
          continue;
        }

        if (!isNewest) {
          continue;
        }

        const restored = confirmed();
        applyRoot(restored);
        setDesired(restored);
        showToast('error', commandFailureMessage(exit.cause, 'Could not save appearance'));
      }
    } finally {
      writeInFlight = false;
      if (!disposed && queued) {
        void runWriteLoop();
      } else if (!disposed) {
        setSaving(false);
      }
    }
  };

  const selectAppearance = (appearance: Appearance) => {
    if (appearancesEqual(appearance, desired()) && queued === null && !writeInFlight) {
      return;
    }
    setDesired(appearance);
    applyRoot(appearance);
    queued = appearance;
    void runWriteLoop();
  };

  const value: AppearanceContextValue = {
    desired,
    confirmed,
    saving,
    selectAppearance,
    selectDesignTheme: (designTheme) => {
      selectAppearance({ ...desired(), designTheme });
    },
    selectColorMode: (colorMode) => {
      selectAppearance({ ...desired(), colorMode });
    },
  };

  return <AppearanceContext.Provider value={value}>{props.children}</AppearanceContext.Provider>;
}

export function useAppearance(): AppearanceContextValue {
  const ctx = useContext(AppearanceContext);
  if (!ctx) {
    throw new Error('useAppearance must be used within an AppearanceProvider');
  }
  return ctx;
}
