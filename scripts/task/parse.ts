import type { CrateShortName } from './crates';
import { parseCrates } from './crates';

export type TaskCommand =
  | { readonly _tag: 'help' }
  | { readonly _tag: 'check' }
  | { readonly _tag: 'fmt'; readonly check: boolean }
  | { readonly _tag: 'lint'; readonly fix: boolean }
  | { readonly _tag: 'typecheck' }
  | {
      readonly _tag: 'rust';
      readonly action: 'fmt';
      readonly check: boolean;
      readonly crates: readonly [];
    }
  | {
      readonly _tag: 'rust';
      readonly action: 'check' | 'clippy' | 'test';
      readonly crates: readonly CrateShortName[];
    }
  | { readonly _tag: 'iced'; readonly smoke: boolean; readonly release: boolean }
  | { readonly _tag: 'icedHot' }
  | { readonly _tag: 'api' };

function unknownOption(command: string, option: string): never {
  throw new Error(`Unknown ${command} option: ${option}`);
}

function expectNoArguments(command: string, args: readonly string[]): void {
  const [argument] = args;
  if (argument !== undefined) unknownOption(command, argument);
}

function parseRust(args: readonly string[]): TaskCommand {
  const [action, ...rest] = args;
  if (action === 'fmt') {
    let check = false;
    for (const argument of rest) {
      if (argument === '--check') check = true;
      else unknownOption('rust fmt', argument);
    }
    return { _tag: 'rust', action, check, crates: [] };
  }
  if (action === 'check' || action === 'clippy' || action === 'test') {
    return { _tag: 'rust', action, crates: parseCrates(rest) };
  }
  throw new Error(
    action === undefined ? 'Missing Rust command.' : `Unknown Rust command: ${action}`,
  );
}

export function parseCli(argv: readonly string[]): TaskCommand {
  const [command, ...args] = argv;
  if (command === undefined || command === 'help' || command === '--help' || command === '-h') {
    return { _tag: 'help' };
  }
  if (command === 'check' || command === 'typecheck' || command === 'api') {
    expectNoArguments(command, args);
    return { _tag: command };
  }
  if (command === 'fmt') {
    let check = false;
    for (const argument of args) {
      if (argument === '--check') check = true;
      else unknownOption(command, argument);
    }
    return { _tag: command, check };
  }
  if (command === 'lint') {
    let fix = false;
    for (const argument of args) {
      if (argument === '--fix') fix = true;
      else unknownOption(command, argument);
    }
    return { _tag: command, fix };
  }
  if (command === 'rust') return parseRust(args);
  if (command === 'iced') {
    const [action, ...rest] = args;
    if (action === 'hot') {
      expectNoArguments('iced hot', rest);
      return { _tag: 'icedHot' };
    }
    if (action !== 'run') {
      throw new Error(
        action === undefined ? 'Missing iced command.' : `Unknown iced command: ${action}`,
      );
    }
    let smoke = false;
    let release = false;
    for (const argument of rest) {
      if (argument === '--smoke') smoke = true;
      else if (argument === '--release') release = true;
      else unknownOption('iced run', argument);
    }
    return { _tag: command, smoke, release };
  }
  throw new Error(`Unknown task command: ${command}`);
}
