import { createFileRoute, useNavigate } from '@tanstack/solid-router';
import LoginPage from '../components/LoginPage';
import {
  AUTHENTICATED_HOME_ROUTE,
  redirectLoggedInUsersToLibrary,
} from '../router-guards';

export const Route = createFileRoute('/login')({
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
