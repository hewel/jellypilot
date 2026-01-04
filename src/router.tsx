import { createRouter } from '@tanstack/solid-router';
import { routeTree } from './routeTree.gen';

// Create router instance
export const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
});

// Register router for type safety
declare module '@tanstack/solid-router' {
  interface Register {
    router: typeof router;
  }
}
