import type { CrateShortName } from './crates';
import { parseCrates } from './crates';
import { MONITOR_DEFAULT_INTERVAL_MS, MONITOR_DEFAULT_SAMPLES } from './monitor';

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
  | {
      readonly _tag: 'monitor';
      readonly pid: number;
      readonly samples: number;
      readonly intervalMs: number;
      readonly output: string;
      readonly label: string | null;
    }
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
  if (command === 'monitor') return parseMonitor(args);
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

function parseMonitor(args: readonly string[]): TaskCommand {
  const result = {
    pid: Number.NaN,
    samples: MONITOR_DEFAULT_SAMPLES,
    label: null as string | null,
    intervalMs: MONITOR_DEFAULT_INTERVAL_MS,
    output: '',
  };
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === '--pid') {
      const value = args[index + 1];
      if (value === undefined) throw new Error('Missing monitor pid.');
      result.pid = Number(value);
      index += 1;
    } else if (argument === '--samples') {
      const value = args[index + 1];
      if (value === undefined) throw new Error('Missing monitor samples.');
      result.samples = Number(value);
      index += 1;
    } else if (argument === '--interval-ms') {
      const value = args[index + 1];
      if (value === undefined) throw new Error('Missing monitor interval.');
      result.intervalMs = Number(value);
      index += 1;
    } else if (argument === '--out') {
      const value = args[index + 1];
      if (value === undefined) throw new Error('Missing monitor output.');
      result.output = value;
      index += 1;
    } else if (argument === '--label') {
      const value = args[index + 1];
      if (value === undefined) throw new Error('Missing monitor label.');
      result.label = value;
      index += 1;
    } else if (argument === undefined) {
      throw new Error('Missing monitor argument.');
    } else {
      unknownOption('monitor', argument);
    }
  }
  if (!Number.isInteger(result.pid) || result.pid <= 0) {
    throw new Error('Monitor requires --pid with a positive process id.');
  }
  if (!Number.isInteger(result.samples) || result.samples <= 0) {
    throw new Error('Monitor requires --samples with a positive integer.');
  }
  if (!Number.isInteger(result.intervalMs) || result.intervalMs <= 0) {
    throw new Error('Monitor requires --interval-ms with a positive integer.');
  }
  if (result.output.length === 0) throw new Error('Monitor requires --out.');
  return { _tag: 'monitor', ...result };
}
