import { Data } from 'effect';

import type { CommandErrorCode } from '../bindings';

export class InvalidServerUrl extends Data.TaggedError('InvalidServerUrl')<{
  readonly message: string;
}> {}

export class StorageParseError extends Data.TaggedError('StorageParseError')<{
  readonly message: string;
  readonly key: string;
}> {}

export class CommandError extends Data.TaggedError('CommandError')<{
  readonly code?: CommandErrorCode;
  readonly message: string;
}> {}

export class AppearanceCanvasError extends Data.TaggedError('AppearanceCanvasError')<{
  readonly message: string;
}> {}
