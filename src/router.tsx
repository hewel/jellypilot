import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
  useNavigate,
} from '@tanstack/solid-router';
import LoginPage from './components/LoginPage';
import OperationsConsole from './components/OperationsConsole';
import { canAccessConsole, checkAuthWithRestore } from './sessionAccess';

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
