import { Effect, Exit } from 'effect';
import { render } from 'solid-js/web';
import {
  applyAppearanceRootAttributes,
  bootstrapAppearance,
  CONTROL_ROOM_DARK_APPEARANCE,
  CONTROL_ROOM_DARK_CANVAS,
  notifyAppearanceReady,
  resolveComputedCanvas,
  type BootstrappedAppearance,
} from '~effects/appearance';

import App from './App';

let mounted = false;

function recoverControlRoomDark(): BootstrappedAppearance {
  const root = document.documentElement;
  applyAppearanceRootAttributes(root, CONTROL_ROOM_DARK_APPEARANCE);
  const canvas =
    resolveComputedCanvas(document.body) ?? resolveComputedCanvas(root) ?? CONTROL_ROOM_DARK_CANVAS;
  return {
    appearance: CONTROL_ROOM_DARK_APPEARANCE,
    canvas,
  };
}

export async function mountApplication(): Promise<void> {
  if (mounted) return;

  const root = document.querySelector('#root');
  if (!root) return;

  mounted = true;

  const exit = await Effect.runPromiseExit(bootstrapAppearance);
  const hydrated = Exit.match(exit, {
    onFailure: () => recoverControlRoomDark(),
    onSuccess: (value) => value,
  });

  render(() => <App />, root);

  await Effect.runPromise(
    notifyAppearanceReady(hydrated.appearance, hydrated.canvas).pipe(Effect.ignore),
  );
}
