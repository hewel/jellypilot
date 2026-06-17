import { createRootRoute, Outlet } from '@tanstack/solid-router';

export const Route = createRootRoute({
  component: () => <Outlet />,
});
