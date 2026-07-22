import { createRouter } from '@tanstack/solid-router';
import type { RouterHistory } from '@tanstack/solid-router';

import { routeTree } from './routeTree.gen';

export {
  redirectLegacyConsoleRoute,
  redirectLoggedInUsersToLibrary,
  redirectRootRoute,
  requireAuthenticatedShell,
} from './router-guards';

export function createJellyPilotRouter(history?: RouterHistory) {
  return createRouter({
    defaultPreload: 'intent',
    history,
    routeTree,
    scrollRestoration: true,
    scrollToTopSelectors: ['[data-scroll-restoration-id="app-scroll-viewport"]'],
  });
}

export const router = createJellyPilotRouter();

declare module '@tanstack/solid-router' {
  interface Register {
    router: typeof router;
  }
}
