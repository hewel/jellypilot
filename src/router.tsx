import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
  useNavigate,
} from '@tanstack/solid-router';
import { commands, type SavedSession } from './bindings';
import LoginPage from './components/LoginPage';
import OperationsConsole from './components/OperationsConsole';

const SESSION_STORAGE_KEY = 'jmsr_auth_session';

export function loadSavedSession(): SavedSession | null {
  try {
    const saved = localStorage.getItem(SESSION_STORAGE_KEY);
    if (saved) return JSON.parse(saved) as SavedSession;
  } catch {
    // Ignore parse errors.
  }
  return null;
}

export function saveSession(session: SavedSession): void {
  localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
}

export function clearSavedSession(): void {
  localStorage.removeItem(SESSION_STORAGE_KEY);
}

async function restoreSavedSession(): Promise<boolean> {
  const savedSession = loadSavedSession();
  if (!savedSession) return false;

  const result = await commands.jellyfinRestoreSession(savedSession);
  if (result.status === 'ok') return true;

  clearSavedSession();
  return false;
}

async function checkAuthWithRestore(): Promise<boolean> {
  try {
    if (await commands.jellyfinIsConnected()) return true;
    return await restoreSavedSession();
  } catch {
    return false;
  }
}

async function canAccessConsole(): Promise<boolean> {
  try {
    if (await commands.jellyfinIsConnected()) return true;
  } catch {
    // Fall back to Saved Session check.
  }
  return loadSavedSession() !== null;
}

const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  beforeLoad: async () => {
    if (await canAccessConsole()) {
      throw redirect({ to: '/console' });
    }
  },
  component: LoginRouteComponent,
});

function LoginRouteComponent() {
  const navigate = useNavigate();

  const handleConnected = () => {
    navigate({ to: '/console' });
  };

  return <LoginPage onConnected={handleConnected} />;
}

const consoleRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/console',
  beforeLoad: async () => {
    if (!(await canAccessConsole())) {
      throw redirect({ to: '/login' });
    }
  },
  component: ConsoleRouteComponent,
});

function ConsoleRouteComponent() {
  const navigate = useNavigate();

  const handleSignedOut = () => {
    navigate({ to: '/login' });
  };

  return <OperationsConsole onSignedOut={handleSignedOut} />;
}

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  beforeLoad: async () => {
    if (await checkAuthWithRestore()) {
      throw redirect({ to: '/console' });
    }
    throw redirect({ to: '/login' });
  },
  component: () => null,
});

const routeTree = rootRoute.addChildren([indexRoute, loginRoute, consoleRoute]);

export const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
});

declare module '@tanstack/solid-router' {
  interface Register {
    router: typeof router;
  }
}
