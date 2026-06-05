import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
  redirect,
  useNavigate,
} from '@tanstack/solid-router';
import AuthenticatedShell from './components/AuthenticatedShell';
import LoginPage from './components/LoginPage';
import { DiagnosticsRoute } from './routes/diagnostics';
import { LibraryBrowseRoute } from './routes/library/browse';
import { LibraryLanding } from './routes/library/home';
import { LibraryItemDetailRoute } from './routes/library/item-detail';
import { LibraryShowDetailRoute } from './routes/library/show-detail';
import { NowPlayingRoute } from './routes/now-playing';
import { SettingsRoute } from './routes/settings';
import { canAccessConsole, checkAuthWithRestore } from './sessionAccess';

const AUTHENTICATED_HOME_ROUTE = '/library';
const LEGACY_CONSOLE_TARGET_ROUTE = '/settings';

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  beforeLoad: redirectLoggedInUsersToLibrary,
  component: LoginRouteComponent,
});

function LoginRouteComponent() {
  const navigate = useNavigate();

  const handleConnected = () => {
    navigate({ to: AUTHENTICATED_HOME_ROUTE });
  };

  return <LoginPage onConnected={handleConnected} />;
}

const authenticatedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'authenticated',
  beforeLoad: requireAuthenticatedShell,
  component: AuthenticatedShell,
});

const libraryRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/library',
  component: LibraryLanding,
});

const libraryBrowseRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/library/$collectionType/$libraryId',
  component: LibraryBrowseRoute,
});

const libraryItemDetailRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/library/items/$itemId',
  component: LibraryItemDetailRoute,
});

const libraryShowDetailRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/library/shows/$seriesId',
  component: LibraryShowDetailRoute,
});

const nowPlayingRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/now-playing',
  component: NowPlayingRoute,
});

const settingsRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/settings',
  component: SettingsRoute,
});

const diagnosticsRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/diagnostics',
  component: DiagnosticsRoute,
});

const consoleRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/console',
  beforeLoad: redirectLegacyConsoleRoute,
  component: () => null,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  beforeLoad: redirectRootRoute,
  component: () => null,
});

export async function redirectLoggedInUsersToLibrary() {
  if (await canAccessConsole()) {
    throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
  }
}

export async function requireAuthenticatedShell() {
  if (!(await canAccessConsole())) {
    throw redirect({ to: '/login' });
  }
}

export async function redirectLegacyConsoleRoute() {
  if (!(await canAccessConsole())) {
    throw redirect({ to: '/login' });
  }
  throw redirect({ to: LEGACY_CONSOLE_TARGET_ROUTE });
}

export async function redirectRootRoute() {
  if (await checkAuthWithRestore()) {
    throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
  }
  throw redirect({ to: '/login' });
}

const routeTree = rootRoute.addChildren([
  indexRoute,
  loginRoute,
  authenticatedRoute.addChildren([
    libraryRoute,
    libraryBrowseRoute,
    libraryItemDetailRoute,
    libraryShowDetailRoute,
    nowPlayingRoute,
    settingsRoute,
    diagnosticsRoute,
  ]),
  consoleRoute,
]);

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
