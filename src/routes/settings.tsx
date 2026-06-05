import { useNavigate } from '@tanstack/solid-router';
import OperationsConsole from '../components/OperationsConsole';

export function SettingsRoute() {
  const navigate = useNavigate();

  const handleSignedOut = () => {
    navigate({ to: '/login' });
  };

  return <OperationsConsole onSignedOut={handleSignedOut} />;
}
