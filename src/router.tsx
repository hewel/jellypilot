import { createRouter, type RouterHistory } from '@tanstack/solid-router';
import { routeTree } from './routeTree.gen';

export {
  redirectLegacyConsoleRoute,
  redirectLoggedInUsersToLibrary,
  redirectRootRoute,
  requireAuthenticatedShell,
} from './router-guards';

export function createJmsrRouter(history?: RouterHistory) {
  return createRouter({
    routeTree,
    defaultPreload: 'intent',
    history,
  });
}

export const router = createJmsrRouter();

declare module '@tanstack/solid-router' {
  interface Register {
    router: typeof router;
  }
}
