import path from 'node:path';

export interface TestOptions {
  readonly headed: boolean;
  readonly spec?: string;
}

export type E2eCommand = 'build' | 'clean' | 'isolation' | 'test' | 'typecheck' | 'verify';

function isE2eCommand(value: string): value is E2eCommand {
  return (
    value === 'build' ||
    value === 'clean' ||
    value === 'isolation' ||
    value === 'test' ||
    value === 'typecheck' ||
    value === 'verify'
  );
}

export function parseCli(argv: readonly string[]): { command: E2eCommand; options: TestOptions } {
  const [rawCommand = 'test', ...rest] = argv;
  if (!isE2eCommand(rawCommand)) {
    throw new Error(`Unknown E2E command: ${rawCommand}`);
  }

  let headed = false;
  let spec: string | undefined;
  for (let index = 0; index < rest.length; index += 1) {
    const argument = rest[index];
    if (argument === '--headed') {
      headed = true;
    } else if (argument === '--spec') {
      const value = rest[index + 1];
      if (!value) throw new Error('--spec requires a path.');
      spec = path.normalize(value);
      index += 1;
    } else {
      throw new Error(`Unknown E2E option: ${argument}`);
    }
  }

  return { command: rawCommand, options: { headed, spec } };
}
