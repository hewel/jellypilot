import { Data } from 'effect';

export class TaskCliError extends Data.TaggedError('TaskCliError')<{
  readonly message: string;
}> {}

export class TaskProcessError extends Data.TaggedError('TaskProcessError')<{
  readonly command: string;
  readonly exitCode: number | null;
  readonly message: string;
}> {}

export class TaskCheckError extends Data.TaggedError('TaskCheckError')<{
  readonly failedSteps: readonly string[];
  readonly message: string;
}> {}
