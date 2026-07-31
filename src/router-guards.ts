import { redirect } from '@tanstack/solid-router';

import { canAccessConsole, resolveAuthenticatedEntry } from './sessionAccess';

export const AUTHENTICATED_HOME_ROUTE = '/library';

export async function redirectLoggedInUsersToLibrary() {
  if (await canAccessConsole()) {
    throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
  }
}

export async function requireAuthenticatedShell() {
  if (!(await resolveAuthenticatedEntry())) {
    throw redirect({ to: '/login' });
  }
}

export async function redirectRootRoute() {
  if (await resolveAuthenticatedEntry()) {
    throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
  }
  throw redirect({ to: '/login' });
}
