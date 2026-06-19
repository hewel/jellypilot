import { Exit } from 'effect';

export const defaultTo =
  <V>(value: V) =>
  <A, E>(exit: Exit.Exit<A, E>): A | V =>
    Exit.match(exit, {
      onFailure: () => value,
      onSuccess: (success) => success,
    });
