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
  | {
      readonly _tag: 'iced';
      readonly smoke: boolean;
      readonly release: boolean;
      readonly embedded: boolean;
    }
  | { readonly _tag: 'mpvBuild'; readonly source: string | null }
  | { readonly _tag: 'icedHot' }
  | { readonly _tag: 'icedPrepare'; readonly source: string | null }
  | { readonly _tag: 'icedBuild'; readonly release: boolean }
  | {
      readonly _tag: 'icedRegress';
      readonly scenario: 'tray' | 'external' | 'gpu' | 'all';
      readonly file: string | null;
      readonly output: string;
    }
  | {
      readonly _tag: 'icedLocalVideo';
      readonly action: 'run';
      readonly smoke: boolean;
      readonly release: boolean;
      readonly file: string | null;
      readonly url: string | null;
      readonly urlFromEnv: boolean;
    }
  | {
      readonly _tag: 'icedLocalVideo';
      readonly action: 'check' | 'test' | 'clippy';
      readonly testFilter: string | null;
    }
  | { readonly _tag: 'icedLocalVideo'; readonly action: 'fmt'; readonly check: boolean }
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

function parseLocalVideo(args: readonly string[]): TaskCommand {
  const [action, ...rest] = args;
  if (action === 'run') {
    let smoke = false;
    let release = false;
    let file: string | null = null;
    let url: string | null = null;
    let urlFromEnv = false;
    for (let index = 0; index < rest.length; index += 1) {
      const argument = rest[index];
      if (argument === '--smoke') smoke = true;
      else if (argument === '--release') release = true;
      else if (argument === '--file' || argument === '--url' || argument === '--url-env') {
        if (file !== null || url !== null || urlFromEnv) {
          throw new Error('Specify exactly one --file, --url or --url-env source.');
        }
        if (argument === '--url-env') {
          urlFromEnv = true;
          continue;
        }
        const value = rest[index + 1];
        if (value === undefined || value.trim().length === 0 || value.startsWith('--')) {
          throw new Error(`iced local-video run ${argument} requires a non-empty value.`);
        }
        if (argument === '--file') file = value;
        else url = value;
        index += 1;
      } else if (argument !== undefined) {
        throw new Error('Unknown iced local-video run option.');
      }
    }
    return { _tag: 'icedLocalVideo', action, smoke, release, file, url, urlFromEnv };
  }
  if (action === 'check' || action === 'clippy') {
    if (rest.length > 0) throw new Error(`Unexpected iced local-video ${action} arguments.`);
    return { _tag: 'icedLocalVideo', action, testFilter: null };
  }
  if (action === 'test') {
    const [filter, ...extra] = rest;
    if (filter !== undefined && (filter.length === 0 || filter.startsWith('-'))) {
      throw new Error('Invalid iced local-video test filter.');
    }
    if (extra.length > 0) throw new Error('Unexpected iced local-video test arguments.');
    return { _tag: 'icedLocalVideo', action, testFilter: filter ?? null };
  }
  if (action === 'fmt') {
    let check = false;
    for (const argument of rest) {
      if (argument === '--check') check = true;
      else throw new Error('Unknown iced local-video fmt option.');
    }
    return { _tag: 'icedLocalVideo', action, check };
  }
  throw new Error(
    action === undefined
      ? 'Missing iced local-video command.'
      : 'Unknown iced local-video command.',
  );
}

function parseSourceOptions(command: string, rest: readonly string[]): string | null {
  let source: string | null = null;
  for (let index = 0; index < rest.length; index += 1) {
    const argument = rest[index];
    if (argument !== '--source') unknownOption(command, argument);
    if (source !== null) throw new Error('Specify --source only once.');
    const value = rest[index + 1];
    if (value === undefined || value.trim().length === 0 || value.startsWith('-')) {
      throw new Error(`${command} --source requires a non-empty checkout path.`);
    }
    source = value;
    index += 1;
  }
  return source;
}

function parseRegression(args: readonly string[]): TaskCommand {
  const [scenario, ...rest] = args;
  if (scenario !== 'tray' && scenario !== 'external' && scenario !== 'gpu' && scenario !== 'all') {
    throw new Error(
      'Expected iced regress <tray|external|gpu|all> [--file <media>] [--out <report-dir>].',
    );
  }
  let file: string | null = null;
  let output = 'target/native-regression';
  const seen = new Set<string>();
  for (let index = 0; index < rest.length; index += 2) {
    const option = rest[index];
    if (option !== '--file' && option !== '--out') {
      throw new Error(`Unknown iced regress option: ${option}`);
    }
    const value = rest[index + 1];
    if (seen.has(option) || value === undefined || value.trim() === '' || value.startsWith('--')) {
      throw new Error(`iced regress requires one non-empty value for ${option}.`);
    }
    seen.add(option);
    if (option === '--file') file = value;
    else output = value;
  }
  if ((scenario === 'gpu' || scenario === 'all') && file === null) {
    throw new Error(
      'iced regress gpu/all requires --file <media>; no media or color coverage is fabricated.',
    );
  }
  if (file !== null && scenario !== 'gpu' && scenario !== 'all') {
    throw new Error('--file applies only to iced regress gpu/all.');
  }
  return { _tag: 'icedRegress', scenario, file, output };
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
  if (command === 'mpv') {
    const [action, ...rest] = args;
    if (action !== 'build') throw new Error('Expected mpv build [--source <checkout>].');
    return { _tag: 'mpvBuild', source: parseSourceOptions('mpv build', rest) };
  }
  if (command === 'monitor') return parseMonitor(args);
  if (command === 'iced') {
    const [action, ...rest] = args;
    if (action === 'local-video') return parseLocalVideo(rest);
    if (action === 'regress') return parseRegression(rest);
    if (action === 'prepare') {
      return { _tag: 'icedPrepare', source: parseSourceOptions('iced prepare', rest) };
    }
    if (action === 'build') {
      for (const argument of rest) {
        if (argument !== '--release') unknownOption('iced build', argument);
      }
      return { _tag: 'icedBuild', release: rest.includes('--release') };
    }
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
    let embedded = false;
    for (const argument of rest) {
      if (argument === '--smoke') smoke = true;
      else if (argument === '--release') release = true;
      else if (argument === '--embedded') embedded = true;
      else unknownOption('iced run', argument);
    }
    return { _tag: command, smoke, release, embedded };
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
