import { createFileRoute } from '@tanstack/solid-router';
import { redirectLegacyConsoleRoute } from '../router-guards';

export const Route = createFileRoute('/console')({
  beforeLoad: redirectLegacyConsoleRoute,
  component: () => null,
});
