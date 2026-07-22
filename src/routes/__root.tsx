import {
  AppScrollAreaProvider,
  createAppScrollAreaController,
} from '@components/AppScrollAreaContext';
import {
  Outlet,
  createRootRoute,
  useElementScrollRestoration,
  useRouter,
} from '@tanstack/solid-router';
import { createEffect, createSignal, onCleanup } from 'solid-js';
import type { Component } from 'solid-js';

import * as styles from './__root.styles';

const ScrollerWrapper: Component = () => {
  const appScroll = createAppScrollAreaController();
  const router = useRouter();
  onCleanup(() => appScroll.setViewport(null));

  /* TanStack applies cached scroll positions synchronously during `onRendered`.
   * That runs before the destination route's DOM has committed, so the browser
   * clamps the assignment whenever the outgoing page is shorter than the
   * restored offset. Re-apply the cached entry after the commit so Back keeps
   * this nested viewport's position even on short origin pages. Read the entry
   * through `useElementScrollRestoration` inside an effect so the Solid owner
   * is available and the lookup resolves against the same router-core module
   * instance the router internals use. */
  const [renderedTick, setRenderedTick] = createSignal(0);
  onCleanup(
    router.subscribe('onRendered', () => {
      requestAnimationFrame(() => setRenderedTick((tick) => tick + 1));
    }),
  );
  createEffect(() => {
    renderedTick();
    const viewport = appScroll.viewport();
    const entry = useElementScrollRestoration({ id: 'app-scroll-viewport' });
    if (viewport && entry) {
      viewport.scrollTo({ left: entry.scrollX, top: entry.scrollY });
    }
  });

  return (
    <AppScrollAreaProvider value={appScroll}>
      <div
        ref={appScroll.setViewport}
        onScroll={appScroll.handleViewportScroll}
        data-testid="app-scroll-viewport"
        data-scroll-restoration-id="app-scroll-viewport"
        class={styles.viewport}
      >
        <div class={styles.content}>
          <Outlet />
        </div>
      </div>
    </AppScrollAreaProvider>
  );
};

export const Route = createRootRoute({
  component: ScrollerWrapper,
});
