import { commands } from '@bindings';
import type { AppLocalServices } from '@bindings';
import type { Effect } from 'effect';

import { runTauriCommandRaw } from './commands';
import type { CommandError } from './errors';

export const fetchAppLocalServices: Effect.Effect<AppLocalServices, CommandError> =
  runTauriCommandRaw(() => commands.appLocalServices());
