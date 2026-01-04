import { createFileRoute, redirect } from '@tanstack/solid-router';
import { checkAuth } from '../lib/auth';

export const Route = createFileRoute('/')({
  beforeLoad: async () => {
    const isAuth = await checkAuth();
    if (isAuth) {
      throw redirect({ to: '/settings' });
    }
    throw redirect({ to: '/login' });
  },
  component: () => null,
});
