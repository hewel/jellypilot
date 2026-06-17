import { redirect } from '@tanstack/solid-router';
import { canAccessConsole, checkAuthWithRestore } from './sessionAccess';

export const AUTHENTICATED_HOME_ROUTE = '/library';
export const LEGACY_CONSOLE_TARGET_ROUTE = '/settings';

export async function redirectLoggedInUsersToLibrary() {
  if (await canAccessConsole()) {
    throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
  }
}

export async function requireAuthenticatedShell() {
  if (!(await canAccessConsole())) {
    throw redirect({ to: '/login' });
  }
}

export async function redirectLegacyConsoleRoute() {
  if (!(await canAccessConsole())) {
    throw redirect({ to: '/login' });
  }
  throw redirect({ to: LEGACY_CONSOLE_TARGET_ROUTE });
}

export async function redirectRootRoute() {
  if (await checkAuthWithRestore()) {
    throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
  }
  throw redirect({ to: '/login' });
}
