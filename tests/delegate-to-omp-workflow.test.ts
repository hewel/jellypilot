import { readFileSync } from 'node:fs';

import { expect, test } from '@rstest/core';

import {
  buildOmpArgs,
  compareSnapshots,
  validatePacket,
  validateReport,
  validateRepositoryDelta,
} from '../.agents/skills/delegate-to-omp/scripts/run-omp-task.mjs';

const packet = () =>
  validatePacket({
    task_id: 'focused-edit',
    title: 'Focused edit',
    mode: 'edit',
    objective: 'Make one bounded change',
    context_paths: ['src/example.ts'],
    write_scope: ['src/example.ts', 'tests/example/'],
    requirements: ['Preserve the public interface'],
    acceptance_criteria: ['The focused behavior is covered'],
    verification: [{ command: 'bun run test -- example', expected: 'Exit 0' }],
    skills: ['solidjs'],
    extra_tools: [],
    timeout_seconds: 120,
  });

test('task packets require bounded writes and exact verification', () => {
  const valid = packet();
  expect(valid.write_scope).toEqual(['src/example.ts', 'tests/example/']);
  expect(valid.verification[0]?.command).toBe('bun run test -- example');

  expect(() =>
    validatePacket({
      ...valid,
      mode: 'verify',
      write_scope: ['src/example.ts'],
    }),
  ).toThrow('verify packets require an empty write_scope');
  expect(() => validatePacket({ ...valid, verification: [] })).toThrow(
    'verification must be a non-empty array',
  );
  expect(() => validatePacket({ ...valid, skills: ['delegate-to-omp'] })).toThrow(
    'Invalid or recursive OMP skill selection',
  );
});

test('OMP launch arguments preserve Codex authority boundaries', () => {
  const args = buildOmpArgs(packet(), '/repo');
  expect(args).toContain('--mode');
  expect(args).toContain('rpc');
  expect(args).toContain('--no-session');
  expect(args).toContain('yolo');
  expect(args).toContain('read,bash,grep,glob,lsp,edit,write');
  expect(args).toContain('solidjs');
  expect(args).not.toContain('task');
  expect(args).not.toContain('ask');

  const verifyArgs = buildOmpArgs(
    validatePacket({
      ...packet(),
      mode: 'verify',
      write_scope: [],
      skills: [],
    }),
    '/repo',
  );
  expect(verifyArgs).toContain('read,bash,grep,glob,lsp');
  expect(verifyArgs).not.toContain('read,bash,grep,glob,lsp,edit,write');
  expect(verifyArgs).toContain('--no-skills');
});

test('OMP reports must preserve the prescribed verification commands', () => {
  const task = packet();
  const validReport = {
    task_id: task.task_id,
    status: 'completed',
    summary: 'Made the bounded change',
    changed_files: ['src/example.ts'],
    verification: [
      {
        command: task.verification[0]?.command,
        exit_code: 0,
        passed: true,
        evidence: 'Focused test passed',
      },
    ],
    deviations: [],
    risks: [],
  };

  expect(validateReport(validReport, task).status).toBe('completed');
  expect(() =>
    validateReport(
      {
        ...validReport,
        verification: [{ ...validReport.verification[0], command: 'bun run test' }],
      },
      task,
    ),
  ).toThrow('substituted or reordered');
  expect(() =>
    validateReport(
      {
        ...validReport,
        verification: [{ ...validReport.verification[0], exit_code: 1, passed: false }],
      },
      task,
    ),
  ).toThrow('reported completed with failed');
});

test('repository guards detect actual deltas without reverting them', () => {
  const before = {
    head: 'head',
    index_hash: 'index',
    status: ' M src/example.ts',
    files: {
      'src/example.ts': 'file:before',
      'src/outside.ts': 'file:stable',
    },
  };
  const allowedAfter = {
    ...before,
    files: { ...before.files, 'src/example.ts': 'file:after' },
  };

  expect(compareSnapshots(before, allowedAfter).changedFiles).toEqual(['src/example.ts']);
  expect(
    validateRepositoryDelta(packet(), before, allowedAfter, ['src/example.ts']).changedFiles,
  ).toEqual(['src/example.ts']);

  const outsideAfter = {
    ...before,
    files: { ...before.files, 'src/outside.ts': 'file:changed' },
  };
  expect(() => validateRepositoryDelta(packet(), before, outsideAfter, ['src/outside.ts'])).toThrow(
    'out-of-scope files changed',
  );

  const verifyTask = validatePacket({ ...packet(), mode: 'verify', write_scope: [] });
  expect(() =>
    validateRepositoryDelta(verifyTask, before, allowedAfter, ['src/example.ts']),
  ).toThrow('verify task changed repository files');
});

test('runner uses Herdr socket calls and structured OMP RPC instead of pane scraping', () => {
  const runner = readFileSync('.agents/skills/delegate-to-omp/scripts/run-omp-task.mjs', 'utf8');
  expect(runner).toContain("'agent.start'");
  expect(runner).toContain("'pane.report_agent'");
  expect(runner).toContain("'pane.close'");
  expect(runner).toContain("type: 'get_last_assistant_text'");
  expect(runner).toContain("frame.type === 'agent_end'");
  expect(runner).not.toContain("['pane', 'read'");
  expect(runner).not.toContain("['wait', 'output'");
});

test('project guidance makes Codex the planner and OMP the bounded executor', () => {
  const agents = readFileSync('AGENTS.md', 'utf8');
  const skill = readFileSync('.agents/skills/delegate-to-omp/SKILL.md', 'utf8');
  expect(agents).toContain('## Role: Codex Planner + OMP Executor');
  expect(agents).toContain('use the local `delegate-to-omp` skill');
  expect(skill).toContain('Keep Codex responsible for intent, direction, decomposition');
  expect(skill).toContain('OMP never owns user-facing completion');
});
