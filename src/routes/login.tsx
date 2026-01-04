import { createFileRoute, redirect, useNavigate } from '@tanstack/solid-router';
import { checkAuth } from '../lib/auth';
import LoginPage from '../pages/Login';

export const Route = createFileRoute('/login')({
  beforeLoad: async () => {
    const isAuth = await checkAuth();
    if (isAuth) {
      throw redirect({ to: '/settings' });
    }
  },
  component: LoginRouteComponent,
});

function LoginRouteComponent() {
  const navigate = useNavigate();

  const handleConnected = () => {
    navigate({ to: '/settings' });
  };

  return <LoginPage onConnected={handleConnected} />;
}
