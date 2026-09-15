import { describe, expect, test } from 'bun:test';

import { rustClippyWorkspaceCommands, rustFormatCommands } from './commands';
import { parseCrates, workspacePackages, workspacesFor } from './crates';

describe('crate routing', () => {
  test('rejects unknown crate aliases', () => {
    expect(() => parseCrates(['unknown'])).toThrow();
  });

  test('routes shared crates to the root workspace and iced to the desktop workspace', () => {
    expect(workspacesFor(['core', 'sdk', 'ffi'])).toEqual(['shared']);
    expect(workspacesFor(['mpv-host'])).toEqual(['desktop']);
    expect(workspacesFor(['iced'])).toEqual(['desktop']);
    expect(workspacesFor(['core', 'iced'])).toEqual(['shared', 'desktop']);
    expect(workspacesFor([])).toEqual(['shared', 'desktop']);
  });

  test('maintained package coverage stays inside each workspace', () => {
    const shared = workspacePackages('shared');
    const desktop = workspacePackages('desktop');
    expect(shared).toContain('jellypilot-sdk');
    expect(shared).toContain('jellypilot-ffi');
    expect(shared).not.toContain('jellypilot-iced');
    expect(desktop).toContain('jellypilot-iced');
    expect(desktop).toContain('jellypilot-launcher');
    expect(desktop).not.toContain('jellypilot-core');
  });

  test('workspace-wide fmt and clippy emit one cargo invocation per workspace', () => {
    const fmt = rustFormatCommands(true);
    expect(fmt).toHaveLength(2);
    expect(fmt[0]?.args).toContain('Cargo.toml');
    expect(fmt[1]?.args).toContain('src-iced/Cargo.toml');
    const clippy = rustClippyWorkspaceCommands();
    expect(clippy).toHaveLength(2);
    expect(clippy[0]?.args).toContain('Cargo.toml');
    expect(clippy[1]?.args).toContain('src-iced/Cargo.toml');
  });
});
