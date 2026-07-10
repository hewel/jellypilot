import type { UIRootProps } from '@jellypilot/ui';
import { Link, createRouter } from '@tanstack/solid-router';
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
  });
}

export const router = createJellyPilotRouter();

export const appLinkAdapter: NonNullable<UIRootProps['linkAdapter']> = (props) => (
  <Link
    to={props.href}
    class={props.class}
    target={props.target}
    rel={props.rel}
    aria-label={props['aria-label']}
    data-jp-link=""
  >
    {props.children}
  </Link>
);

declare module '@tanstack/solid-router' {
  interface Register {
    router: typeof router;
  }
}
