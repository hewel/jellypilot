import { Data } from 'effect';

export class InvalidServerUrl extends Data.TaggedError('InvalidServerUrl')<{
  readonly message: string;
}> {}

export class StorageParseError extends Data.TaggedError('StorageParseError')<{
  readonly message: string;
  readonly key: string;
}> {}

export class CommandError extends Data.TaggedError('CommandError')<{
  readonly message: string;
}> {}
