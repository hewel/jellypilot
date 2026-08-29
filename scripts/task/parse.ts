import type { CrateShortName } from './crates';
import { parseCrates } from './crates';

export type E2eSubcommand = 'build' | 'test' | 'typecheck' | 'isolation' | 'verify' | 'clean';

export type TaskCommand =
  | { readonly _tag: 'help' }
  | { readonly _tag: 'dev'; readonly rsdoctor: boolean; readonly skipSetup: boolean }
  | { readonly _tag: 'build'; readonly rsdoctor: boolean; readonly skipSetup: boolean }
  | { readonly _tag: 'preview'; readonly skipSetup: boolean }
  | {
      readonly _tag: 'test';
      readonly watch: boolean;
      readonly all: boolean;
      readonly skipSetup: boolean;
    }
  | { readonly _tag: 'check' }
  | { readonly _tag: 'fmt'; readonly check: boolean }
  | { readonly _tag: 'lint'; readonly fix: boolean }
  | { readonly _tag: 'typecheck'; readonly skipSetup: boolean }
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
  | { readonly _tag: 'wasm'; readonly action: 'install' }
  | { readonly _tag: 'wasm'; readonly action: 'build'; readonly mode?: '--dev' | '--release' }
  | { readonly _tag: 'ffmpeg'; readonly verify: boolean; readonly target?: string }
  | { readonly _tag: 'iced'; readonly smoke: boolean; readonly release: boolean }
  | {
      readonly _tag: 'e2e';
      readonly action: E2eSubcommand;
      readonly skipSetup: boolean;
      readonly args: readonly string[];
    }
  | { readonly _tag: 'api' }
  | { readonly _tag: 'panda' }
  | {
      readonly _tag: 'review';
      readonly action: 'panda-tauri' | 'parity';
      readonly args: readonly string[];
    };

function unknownOption(command: string, option: string): never {
  throw new Error(`Unknown ${command} option: ${option}`);
}

function expectNoArguments(command: string, args: readonly string[]): void {
  const [argument] = args;
  if (argument !== undefined) unknownOption(command, argument);
}

function parseSetupFlags(
  command: 'dev' | 'build',
  args: readonly string[],
): { readonly rsdoctor: boolean; readonly skipSetup: boolean } {
  let rsdoctor = false;
  let skipSetup = false;
  for (const argument of args) {
    if (argument === '--rsdoctor') rsdoctor = true;
    else if (argument === '--skip-setup') skipSetup = true;
    else unknownOption(command, argument);
  }
  return { rsdoctor, skipSetup };
}

function parseE2eSubcommand(value: string | undefined): E2eSubcommand {
  if (
    value === 'build' ||
    value === 'test' ||
    value === 'typecheck' ||
    value === 'isolation' ||
    value === 'verify' ||
    value === 'clean'
  ) {
    return value;
  }
  throw new Error(value === undefined ? 'Missing E2E command.' : `Unknown E2E command: ${value}`);
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

function parseWasm(args: readonly string[]): TaskCommand {
  const [action, ...rest] = args;
  if (action === 'install') {
    expectNoArguments('wasm install', rest);
    return { _tag: 'wasm', action };
  }
  if (action === 'build') {
    const [mode, ...extra] = rest;
    if (mode !== undefined && mode !== '--dev' && mode !== '--release') {
      unknownOption('wasm build', mode);
    }
    expectNoArguments('wasm build', extra);
    return mode === undefined ? { _tag: 'wasm', action } : { _tag: 'wasm', action, mode };
  }
  throw new Error(
    action === undefined ? 'Missing WASM command.' : `Unknown WASM command: ${action}`,
  );
}

export function parseCli(argv: readonly string[]): TaskCommand {
  const [command, ...args] = argv;
  if (command === undefined || command === 'help' || command === '--help' || command === '-h') {
    return { _tag: 'help' };
  }
  if (command === 'dev' || command === 'build') {
    return { _tag: command, ...parseSetupFlags(command, args) };
  }
  if (command === 'preview') {
    let skipSetup = false;
    for (const argument of args) {
      if (argument === '--skip-setup') skipSetup = true;
      else unknownOption(command, argument);
    }
    return { _tag: command, skipSetup };
  }
  if (command === 'test') {
    let watch = false;
    let all = false;
    let skipSetup = false;
    for (const argument of args) {
      if (argument === '--watch') watch = true;
      else if (argument === '--all') all = true;
      else if (argument === '--skip-setup') skipSetup = true;
      else unknownOption(command, argument);
    }
    if (watch && all) throw new Error('Cannot combine --watch with --all.');
    return { _tag: command, watch, all, skipSetup };
  }
  if (command === 'check' || command === 'api') {
    expectNoArguments(command, args);
    return { _tag: command };
  }
  if (command === 'panda') {
    const [action, ...rest] = args;
    if (action !== 'codegen') {
      throw new Error(
        action === undefined ? 'Missing Panda command.' : `Unknown Panda command: ${action}`,
      );
    }
    expectNoArguments('panda codegen', rest);
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
  if (command === 'typecheck') {
    let skipSetup = false;
    for (const argument of args) {
      if (argument === '--skip-setup') skipSetup = true;
      else unknownOption(command, argument);
    }
    return { _tag: command, skipSetup };
  }
  if (command === 'rust') return parseRust(args);
  if (command === 'wasm') return parseWasm(args);
  if (command === 'ffmpeg') {
    const [action, ...rest] = args;
    if (action !== 'prepare') {
      throw new Error(
        action === undefined ? 'Missing FFmpeg command.' : `Unknown FFmpeg command: ${action}`,
      );
    }
    let verify = false;
    let target: string | undefined;
    for (let index = 0; index < rest.length; index += 1) {
      const argument = rest[index];
      if (argument === '--verify') {
        verify = true;
      } else if (argument === '--target') {
        const value = rest[index + 1];
        if (value === undefined) throw new Error('--target requires a Rust target triple.');
        target = value;
        index += 1;
      } else {
        unknownOption('ffmpeg prepare', argument);
      }
    }
    return target === undefined ? { _tag: command, verify } : { _tag: command, verify, target };
  }
  if (command === 'iced') {
    const [action, ...rest] = args;
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
  if (command === 'e2e') {
    const [action, ...rest] = args;
    let skipSetup = false;
    const forwarded: string[] = [];
    for (const argument of rest) {
      if (argument === '--skip-setup') skipSetup = true;
      else forwarded.push(argument);
    }
    return { _tag: command, action: parseE2eSubcommand(action), args: forwarded, skipSetup };
  }
  if (command === 'review') {
    const [action, ...rest] = args;
    if (action !== 'panda-tauri' && action !== 'parity') {
      throw new Error(
        action === undefined ? 'Missing review command.' : `Unknown review command: ${action}`,
      );
    }
    if (action === 'panda-tauri') expectNoArguments('review panda-tauri', rest);
    return { _tag: command, action, args: action === 'parity' ? rest : [] };
  }
  throw new Error(`Unknown task command: ${command}`);
}
