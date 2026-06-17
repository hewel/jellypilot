import { createFileRoute } from '@tanstack/solid-router';
import { redirectRootRoute } from '../router-guards';

export const Route = createFileRoute('/')({
  beforeLoad: redirectRootRoute,
  component: () => null,
});
