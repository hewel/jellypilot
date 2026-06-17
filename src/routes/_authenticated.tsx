import { createFileRoute } from '@tanstack/solid-router';
import AuthenticatedShell from '../components/AuthenticatedShell';
import { requireAuthenticatedShell } from '../router-guards';

export const Route = createFileRoute('/_authenticated')({
  beforeLoad: requireAuthenticatedShell,
  component: AuthenticatedShell,
});
