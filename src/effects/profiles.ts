import { commands } from '@bindings';
import type { SavedServiceProfiles, SavedSession } from '@bindings';
import type { Effect } from 'effect';

import { runTauriCommand } from './commands';
import type { CommandError } from './errors';

export function fetchSavedServiceProfiles(): Effect.Effect<SavedServiceProfiles, CommandError> {
  return runTauriCommand(() => commands.serverProfilesGet());
}

export function importLegacySavedSession(
  session: SavedSession,
): Effect.Effect<SavedServiceProfiles, CommandError> {
  return runTauriCommand(() => commands.serverProfilesImportLegacy(session));
}

export function saveCurrentServiceProfile(): Effect.Effect<SavedServiceProfiles, CommandError> {
  return runTauriCommand(() => commands.serverProfilesSaveCurrent());
}

export function activateSavedServiceProfile(
  key: string,
): Effect.Effect<SavedServiceProfiles, CommandError> {
  return runTauriCommand(() => commands.serverProfilesActivate(key));
}

export function removeSavedServiceProfile(
  key: string,
): Effect.Effect<SavedServiceProfiles, CommandError> {
  return runTauriCommand(() => commands.serverProfilesRemove(key));
}
