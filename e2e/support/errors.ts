import { Data } from 'effect';

export class E2ePreflightError extends Data.TaggedError('E2ePreflightError')<{
  readonly message: string;
}> {}

export class E2eBuildError extends Data.TaggedError('E2eBuildError')<{
  readonly message: string;
  readonly cause?: unknown;
}> {}

export class E2eStaleBuildError extends Data.TaggedError('E2eStaleBuildError')<{
  readonly message: string;
}> {}

export class E2eProcessError extends Data.TaggedError('E2eProcessError')<{
  readonly command: string;
  readonly exitCode: number | null;
  readonly message: string;
  readonly timedOut: boolean;
}> {}

export class E2eOwnershipError extends Data.TaggedError('E2eOwnershipError')<{
  readonly path: string;
  readonly message: string;
}> {}

export class E2eEvidenceError extends Data.TaggedError('E2eEvidenceError')<{
  readonly message: string;
}> {}

export class E2eIsolationError extends Data.TaggedError('E2eIsolationError')<{
  readonly message: string;
}> {}

export class E2eRunError extends Data.TaggedError('E2eRunError')<{
  readonly message: string;
  readonly cause?: unknown;
}> {}
