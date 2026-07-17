#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, mkdir, mkdtemp, readFile, readlink, rm, writeFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

const DEFAULT_TIMEOUT_SECONDS = 1800;
const MAX_TIMEOUT_SECONDS = 7200;
const DEFAULT_VERIFY_TOOLS = ['read', 'bash', 'grep', 'glob', 'lsp'];
const DEFAULT_EDIT_TOOLS = [...DEFAULT_VERIFY_TOOLS, 'edit', 'write'];
const EXTRA_TOOLS = new Set(['browser', 'inspect_image', 'python']);
const RESULT_STATUSES = new Set(['completed', 'blocked', 'failed']);
const MIN_HERDR_PROTOCOL = 16;
let herdrReportSequence = Date.now() * 1000;

class WorkflowError extends Error {
  constructor(message, exitCode = 1) {
    super(message);
    this.name = 'WorkflowError';
    this.exitCode = exitCode;
  }
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function parseCliArgs(argv) {
  let packetPath;
  let keepPane = false;

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--packet') {
      packetPath = argv[index + 1];
      index += 1;
    } else if (argument === '--keep-pane') {
      keepPane = true;
    } else {
      throw new WorkflowError(`Unknown argument: ${argument}`, 2);
    }
  }

  if (!packetPath) {
    throw new WorkflowError('Usage: run-omp-task.mjs --packet <absolute-json> [--keep-pane]', 2);
  }

  return { packetPath: path.resolve(packetPath), keepPane };
}

function requireString(value, field) {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new WorkflowError(`${field} must be a non-empty string`, 2);
  }
  return value.trim();
}

function requireStringArray(value, field, { nonEmpty = false } = {}) {
  if (!Array.isArray(value) || (nonEmpty && value.length === 0)) {
    throw new WorkflowError(`${field} must be ${nonEmpty ? 'a non-empty' : 'an'} array`, 2);
  }
  return value.map((entry, index) => requireString(entry, `${field}[${index}]`));
}

function normalizeRepoPath(value, field) {
  const raw = requireString(value, field).replaceAll('\\', '/');
  const directory = raw.endsWith('/');
  const withoutPrefix = raw.startsWith('./') ? raw.slice(2) : raw;

  if (path.posix.isAbsolute(withoutPrefix) || withoutPrefix.includes('\0')) {
    throw new WorkflowError(`${field} must be repository-relative`, 2);
  }

  const normalized = path.posix.normalize(withoutPrefix);
  if (normalized === '.' || normalized === '..' || normalized.startsWith('../')) {
    throw new WorkflowError(`${field} escapes the repository`, 2);
  }

  return directory ? `${normalized.replace(/\/$/, '')}/` : normalized;
}

function normalizeChangedPath(value, field) {
  const normalized = normalizeRepoPath(value, field);
  if (normalized.endsWith('/')) {
    throw new WorkflowError(`${field} must identify a file`, 2);
  }
  return normalized;
}

export function validatePacket(raw) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new WorkflowError('Task packet must be a JSON object', 2);
  }

  const taskId = requireString(raw.task_id, 'task_id');
  if (!/^[a-z0-9][a-z0-9-]{0,62}$/.test(taskId)) {
    throw new WorkflowError(
      'task_id must be a lowercase kebab-case slug of at most 63 characters',
      2,
    );
  }

  if (raw.mode !== 'edit' && raw.mode !== 'verify') {
    throw new WorkflowError('mode must be either "edit" or "verify"', 2);
  }

  const writeScope = requireStringArray(raw.write_scope, 'write_scope').map((entry, index) =>
    normalizeRepoPath(entry, `write_scope[${index}]`),
  );
  if (raw.mode === 'edit' && writeScope.length === 0) {
    throw new WorkflowError('edit packets require a non-empty write_scope', 2);
  }
  if (raw.mode === 'verify' && writeScope.length > 0) {
    throw new WorkflowError('verify packets require an empty write_scope', 2);
  }

  const verification = raw.verification;
  if (!Array.isArray(verification) || verification.length === 0) {
    throw new WorkflowError('verification must be a non-empty array', 2);
  }

  const skills = raw.skills === undefined ? [] : requireStringArray(raw.skills, 'skills');
  for (const skill of skills) {
    if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(skill) || skill === 'delegate-to-omp') {
      throw new WorkflowError(`Invalid or recursive OMP skill selection: ${skill}`, 2);
    }
  }

  const extraTools =
    raw.extra_tools === undefined ? [] : requireStringArray(raw.extra_tools, 'extra_tools');
  for (const tool of extraTools) {
    if (!EXTRA_TOOLS.has(tool)) {
      throw new WorkflowError(`Unsupported extra tool: ${tool}`, 2);
    }
  }

  const timeoutSeconds = raw.timeout_seconds ?? DEFAULT_TIMEOUT_SECONDS;
  if (
    !Number.isInteger(timeoutSeconds) ||
    timeoutSeconds < 1 ||
    timeoutSeconds > MAX_TIMEOUT_SECONDS
  ) {
    throw new WorkflowError(
      `timeout_seconds must be an integer from 1 to ${MAX_TIMEOUT_SECONDS}`,
      2,
    );
  }

  return {
    task_id: taskId,
    title: requireString(raw.title, 'title'),
    mode: raw.mode,
    objective: requireString(raw.objective, 'objective'),
    context_paths: requireStringArray(raw.context_paths, 'context_paths').map((entry, index) =>
      normalizeRepoPath(entry, `context_paths[${index}]`),
    ),
    write_scope: [...new Set(writeScope)],
    requirements: requireStringArray(raw.requirements, 'requirements', { nonEmpty: true }),
    acceptance_criteria: requireStringArray(raw.acceptance_criteria, 'acceptance_criteria', {
      nonEmpty: true,
    }),
    verification: verification.map((entry, index) => {
      if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
        throw new WorkflowError(`verification[${index}] must be an object`, 2);
      }
      return {
        command: requireString(entry.command, `verification[${index}].command`),
        expected: requireString(entry.expected, `verification[${index}].expected`),
      };
    }),
    skills: [...new Set(skills)],
    extra_tools: [...new Set(extraTools)],
    timeout_seconds: timeoutSeconds,
  };
}

function startCommand(command, args, options = {}) {
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env ?? process.env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';

  child.stdout.setEncoding('utf8');
  child.stderr.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    stdout += chunk;
  });
  child.stderr.on('data', (chunk) => {
    stderr += chunk;
  });

  const promise = new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('close', (code, signal) => {
      resolve({ code: code ?? 1, signal, stdout, stderr });
    });
  });

  return {
    promise,
    kill() {
      if (!child.killed) child.kill('SIGTERM');
    },
  };
}

async function runCommand(command, args, options = {}) {
  const result = await startCommand(command, args, options).promise;
  if (result.code !== 0 && !options.allowFailure) {
    const details = result.stderr.trim() || result.stdout.trim() || `exit ${result.code}`;
    throw new WorkflowError(`${command} ${args.join(' ')} failed: ${details}`);
  }
  return result;
}

async function git(root, args, options = {}) {
  return runCommand('git', args, { cwd: root, ...options });
}

async function hashFile(filePath) {
  const stat = await lstat(filePath).catch((error) => {
    if (error?.code === 'ENOENT') return undefined;
    throw error;
  });
  if (!stat) return 'missing';
  if (stat.isSymbolicLink()) return `symlink:${sha256(await readlink(filePath))}`;
  if (!stat.isFile()) return `other:${stat.mode}`;

  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('error', reject);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('end', () => resolve(`file:${hash.digest('hex')}`));
  });
}

async function mapWithConcurrency(values, concurrency, mapper) {
  const results = Array.from({ length: values.length });
  let nextIndex = 0;

  async function worker() {
    while (nextIndex < values.length) {
      const index = nextIndex;
      nextIndex += 1;
      results[index] = await mapper(values[index], index);
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(concurrency, Math.max(values.length, 1)) }, () => worker()),
  );
  return results;
}

export async function snapshotRepository(root) {
  const [head, indexDiff, status, filesResult] = await Promise.all([
    git(root, ['rev-parse', 'HEAD']),
    git(root, ['diff', '--cached', '--binary', '--no-ext-diff']),
    git(root, ['status', '--short']),
    git(root, ['ls-files', '--cached', '--others', '--exclude-standard', '-z']),
  ]);

  const files = filesResult.stdout.split('\0').filter(Boolean).toSorted();
  const entries = await mapWithConcurrency(files, 32, async (repoPath) => [
    repoPath.replaceAll('\\', '/'),
    await hashFile(path.join(root, repoPath)),
  ]);

  return {
    head: head.stdout.trim(),
    index_hash: sha256(indexDiff.stdout),
    status: status.stdout.trimEnd(),
    files: Object.fromEntries(entries),
  };
}

export function compareSnapshots(before, after) {
  const paths = new Set([...Object.keys(before.files), ...Object.keys(after.files)]);
  const changedFiles = [...paths]
    .filter((repoPath) => before.files[repoPath] !== after.files[repoPath])
    .toSorted();
  return {
    changedFiles,
    headChanged: before.head !== after.head,
    indexChanged: before.index_hash !== after.index_hash,
  };
}

function pathAllowed(repoPath, writeScope) {
  return writeScope.some((entry) =>
    entry.endsWith('/') ? repoPath.startsWith(entry) : repoPath === entry,
  );
}

export function validateRepositoryDelta(packet, before, after, reportedChangedFiles) {
  const delta = compareSnapshots(before, after);
  const problems = [];

  if (delta.headChanged)
    problems.push('OMP changed HEAD; commits and branch movement are forbidden');
  if (delta.indexChanged) problems.push('OMP changed the Git index; staging is forbidden');

  if (packet.mode === 'verify' && delta.changedFiles.length > 0) {
    problems.push(`verify task changed repository files: ${delta.changedFiles.join(', ')}`);
  }
  if (packet.mode === 'edit') {
    const outsideScope = delta.changedFiles.filter(
      (repoPath) => !pathAllowed(repoPath, packet.write_scope),
    );
    if (outsideScope.length > 0) {
      problems.push(`out-of-scope files changed: ${outsideScope.join(', ')}`);
    }
  }

  const observed = delta.changedFiles.toSorted();
  const reported = [...new Set(reportedChangedFiles)].toSorted();
  if (JSON.stringify(observed) !== JSON.stringify(reported)) {
    problems.push(
      `reported changed_files do not match the observed delta (reported: ${reported.join(', ') || 'none'}; observed: ${observed.join(', ') || 'none'})`,
    );
  }

  if (problems.length > 0) {
    throw new WorkflowError(`Repository guard failed:\n- ${problems.join('\n- ')}`, 4);
  }

  return delta;
}

export function validateReport(raw, packet) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new WorkflowError('OMP result must be a JSON object', 4);
  }
  if (raw.task_id !== packet.task_id) {
    throw new WorkflowError('OMP result task_id does not match the packet', 4);
  }
  if (!RESULT_STATUSES.has(raw.status)) {
    throw new WorkflowError('OMP result has an invalid status', 4);
  }

  const changedFiles = requireStringArray(raw.changed_files, 'result.changed_files').map(
    (entry, index) => normalizeChangedPath(entry, `result.changed_files[${index}]`),
  );
  const deviations = requireStringArray(raw.deviations, 'result.deviations');
  const risks = requireStringArray(raw.risks, 'result.risks');

  if (!Array.isArray(raw.verification) || raw.verification.length !== packet.verification.length) {
    throw new WorkflowError('OMP result must report every prescribed verification command', 4);
  }

  const verification = raw.verification.map((entry, index) => {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new WorkflowError(`result.verification[${index}] must be an object`, 4);
    }
    if (entry.command !== packet.verification[index].command) {
      throw new WorkflowError(`OMP substituted or reordered verification command ${index + 1}`, 4);
    }
    if (entry.exit_code !== null && !Number.isInteger(entry.exit_code)) {
      throw new WorkflowError(
        `result.verification[${index}].exit_code must be an integer or null`,
        4,
      );
    }
    if (typeof entry.passed !== 'boolean') {
      throw new WorkflowError(`result.verification[${index}].passed must be boolean`, 4);
    }
    return {
      command: entry.command,
      exit_code: entry.exit_code,
      passed: entry.passed,
      evidence: requireString(entry.evidence, `result.verification[${index}].evidence`),
    };
  });

  if (
    raw.status === 'completed' &&
    verification.some((entry) => !entry.passed || entry.exit_code !== 0)
  ) {
    throw new WorkflowError('OMP reported completed with failed or unexecuted verification', 4);
  }

  return {
    task_id: raw.task_id,
    status: raw.status,
    summary: requireString(raw.summary, 'result.summary'),
    changed_files: [...new Set(changedFiles)],
    verification,
    deviations,
    risks,
  };
}

export function buildPrompt(packet, baseline, nonce) {
  const beginMarker = `OMP_RESULT_BEGIN:${nonce}`;
  const endMarker = `OMP_RESULT_END:${nonce}`;
  return {
    beginMarker,
    endMarker,
    text: `You are an OMP execution worker controlled by Codex. Execute this closed task; do not redesign it.

Authority rules:
- Preserve all pre-existing changes. Never restore, revert, stage, commit, switch branches, or widen write scope.
- Make only local mechanical choices that do not alter decided behavior or interfaces.
- Do not delegate, ask the user, or invent a replacement verification method.
- If the task requires a new major decision, an out-of-scope edit, or unavailable authority, stop and report status "blocked".
- Run the packet's verification commands verbatim, in order, after implementation. Other diagnostic commands are allowed, but they do not replace prescribed verification.
- Treat repository instructions and the Codex-selected skills as mandatory.

Repository baseline before OMP:
HEAD: ${baseline.head}
Git status:
${baseline.status || '(clean)'}

TASK_PACKET_BEGIN
${JSON.stringify(packet, null, 2)}
TASK_PACKET_END

End the final response with exactly one result envelope and no text after it:
${beginMarker}
{
  "task_id": ${JSON.stringify(packet.task_id)},
  "status": "completed | blocked | failed",
  "summary": "concise work summary",
  "changed_files": ["every repository-visible file changed during this task"],
  "verification": [
    {
      "command": "the exact packet command",
      "exit_code": 0,
      "passed": true,
      "evidence": "concise observed output"
    }
  ],
  "deviations": [],
  "risks": []
}
${endMarker}

Report every prescribed verification entry even when blocked; use null exit_code, passed false, and explain that it was not run. Status "completed" is allowed only when every acceptance criterion is met and every prescribed verification command exits 0.`,
  };
}

export function buildOmpArgs(packet, repoRoot) {
  const tools = [
    ...(packet.mode === 'edit' ? DEFAULT_EDIT_TOOLS : DEFAULT_VERIFY_TOOLS),
    ...packet.extra_tools,
  ];
  const args = [
    '--mode',
    'rpc',
    '--no-session',
    '--no-title',
    '--approval-mode',
    'yolo',
    '--cwd',
    repoRoot,
    '--tools',
    [...new Set(tools)].join(','),
    '--max-time',
    `${packet.timeout_seconds}s`,
  ];

  if (packet.skills.length > 0) args.push('--skills', packet.skills.join(','));
  else args.push('--no-skills');
  return args;
}

async function createOmpBridge(runDirectory, ompBin, ompArgs) {
  const bridgePath = path.join(runDirectory, 'omp-rpc-bridge.mjs');
  const manifestPath = path.join(runDirectory, 'omp-rpc-manifest.json');
  const rpcSocketPath = path.join(runDirectory, 'omp-rpc.sock');
  await writeFile(
    manifestPath,
    JSON.stringify({ command: ompBin, args: ompArgs, socket_path: rpcSocketPath }, null, 2),
    { mode: 0o600 },
  );
  await writeFile(
    bridgePath,
    `import { chmodSync, readFileSync, rmSync } from 'node:fs';
import process from 'node:process';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';

const manifest = JSON.parse(readFileSync(process.argv[2], 'utf8'));
let child;
let connected = false;

function holdPane() {
  setInterval(() => {}, 60_000);
}

try {
  rmSync(manifest.socket_path, { force: true });
} catch {}

const server = createServer((socket) => {
  if (connected) {
    socket.end(JSON.stringify({ type: 'bridge_error', message: 'RPC client already connected' }) + '\\n');
    return;
  }
  connected = true;
  console.log('[delegate-to-omp] RPC client connected; starting OMP');
  child = spawn(manifest.command, manifest.args, {
    env: process.env,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  socket.pipe(child.stdin);

  let stdoutBuffer = '';
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    socket.write(chunk);
    stdoutBuffer += chunk;
    while (true) {
      const newline = stdoutBuffer.indexOf('\\n');
      if (newline < 0) break;
      const line = stdoutBuffer.slice(0, newline);
      stdoutBuffer = stdoutBuffer.slice(newline + 1);
      try {
        const frame = JSON.parse(line);
        if (frame.type === 'ready') console.log('[delegate-to-omp] OMP RPC ready');
        if (frame.type === 'tool_execution_start') {
          console.log('[delegate-to-omp] tool:', frame.toolName || 'unknown');
        }
        if (frame.type === 'agent_end') console.log('[delegate-to-omp] OMP turn complete');
      } catch {}
    }
  });
  child.stderr.pipe(process.stderr);

  child.on('error', (error) => {
    socket.write(JSON.stringify({ type: 'bridge_error', message: error.message }) + '\\n');
    console.error('[delegate-to-omp] Could not start OMP:', error.message);
  });
  child.on('close', (code) => {
    socket.end(JSON.stringify({ type: 'bridge_exit', code: code ?? 1 }) + '\\n');
    console.log('[delegate-to-omp] OMP exited with code', code ?? 1);
    server.close();
    holdPane();
  });
  socket.on('end', () => child.stdin.end());
  socket.on('error', () => child.stdin.end());
});

server.listen(manifest.socket_path, () => {
  chmodSync(manifest.socket_path, 0o600);
  console.log('[delegate-to-omp] RPC bridge ready');
});
server.on('error', (error) => {
  console.error('[delegate-to-omp] RPC bridge failed:', error.message);
  holdPane();
});

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => {
    child?.kill(signal);
    server.close();
    process.exit(0);
  });
}
`,
    { mode: 0o600 },
  );
  return { bridgePath, manifestPath, rpcSocketPath };
}

function extractReport(transcript, beginMarker, endMarker) {
  const start = transcript.lastIndexOf(beginMarker);
  const end = transcript.indexOf(endMarker, start + beginMarker.length);
  if (start === -1 || end === -1) {
    throw new WorkflowError('OMP output did not contain the required result envelope', 4);
  }
  const json = transcript.slice(start + beginMarker.length, end).trim();
  try {
    return JSON.parse(json);
  } catch (error) {
    throw new WorkflowError(`OMP result JSON is malformed: ${error.message}`, 4);
  }
}

function renderReport(report, paneId, reportPath) {
  const lines = [
    `OMP task ${report.task_id}: ${report.status}`,
    `Summary: ${report.summary}`,
    `Changed files: ${report.changed_files.join(', ') || 'none'}`,
    'Verification:',
    ...report.verification.map(
      (entry) =>
        `- ${entry.passed ? 'PASS' : 'FAIL'} (${entry.exit_code ?? 'not run'}) ${entry.command}: ${entry.evidence}`,
    ),
    `Deviations: ${report.deviations.join('; ') || 'none'}`,
    `Risks: ${report.risks.join('; ') || 'none'}`,
    `Herdr pane: ${paneId}`,
    `Structured report: ${reportPath}`,
  ];
  return lines.join('\n');
}

function herdrRequest(socketPath, method, params, timeoutMilliseconds = 5000) {
  const id = `delegate-to-omp:${method}:${randomBytes(8).toString('hex')}`;
  return new Promise((resolve, reject) => {
    const socket = createConnection(socketPath);
    let buffer = '';
    let settled = false;
    const timer = setTimeout(
      () => finish(new WorkflowError(`Herdr ${method} timed out`, 3)),
      timeoutMilliseconds,
    );

    function finish(error, result) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      if (error) reject(error);
      else resolve(result);
    }

    socket.setEncoding('utf8');
    socket.on('connect', () => socket.write(`${JSON.stringify({ id, method, params })}\n`));
    socket.on('data', (chunk) => {
      buffer += chunk;
      while (true) {
        const newline = buffer.indexOf('\n');
        if (newline === -1) return;
        const line = buffer.slice(0, newline);
        buffer = buffer.slice(newline + 1);
        let frame;
        try {
          frame = JSON.parse(line);
        } catch {
          continue;
        }
        if (frame.id !== id) continue;
        if (frame.error) {
          finish(
            new WorkflowError(
              `Herdr ${method} failed: ${frame.error.code ?? 'error'}: ${frame.error.message ?? 'unknown error'}`,
              3,
            ),
          );
        } else {
          finish(undefined, frame.result);
        }
        return;
      }
    });
    socket.on('error', (error) =>
      finish(new WorkflowError(`Herdr socket error: ${error.message}`, 3)),
    );
    socket.on('end', () =>
      finish(new WorkflowError(`Herdr closed the socket during ${method}`, 3)),
    );
  });
}

async function prepareHerdr(socketPath, callerPaneId) {
  const pong = await herdrRequest(socketPath, 'ping', {});
  if (
    pong?.type !== 'pong' ||
    !Number.isInteger(pong.protocol) ||
    pong.protocol < MIN_HERDR_PROTOCOL
  ) {
    throw new WorkflowError(
      `Herdr protocol ${pong?.protocol ?? 'unknown'} is too old; protocol ${MIN_HERDR_PROTOCOL}+ is required`,
      2,
    );
  }
  const current = await herdrRequest(socketPath, 'pane.current', {
    caller_pane_id: callerPaneId,
  });
  const workspaceId = current?.pane?.workspace_id;
  if (typeof workspaceId !== 'string') {
    throw new WorkflowError('Could not determine the current Herdr workspace', 2);
  }
  return workspaceId;
}

async function startOmpAgent(
  herdrSocketPath,
  packet,
  repoRoot,
  workspaceId,
  bridgePath,
  manifestPath,
  nonce,
) {
  const result = await herdrRequest(herdrSocketPath, 'agent.start', {
    name: `omp-${packet.task_id}-${nonce.slice(0, 8)}`,
    argv: [process.execPath, bridgePath, manifestPath],
    cwd: repoRoot,
    workspace_id: workspaceId,
    focus: false,
  });
  const paneId = result?.agent?.pane_id;
  if (typeof paneId !== 'string') {
    throw new WorkflowError('Herdr agent.start did not return a pane id', 3);
  }
  return paneId;
}

async function reportHerdrAgent(herdrSocketPath, paneId, state, message) {
  herdrReportSequence += 1;
  await herdrRequest(herdrSocketPath, 'pane.report_agent', {
    pane_id: paneId,
    source: 'jellypilot:delegate-to-omp',
    agent: 'omp',
    state,
    message: message || null,
    seq: herdrReportSequence,
  });
}

class RpcChannel {
  constructor(socket) {
    this.socket = socket;
    this.buffer = '';
    this.history = [];
    this.waiters = new Set();
    socket.setEncoding('utf8');
    socket.on('data', (chunk) => this.onData(chunk));
    socket.on('error', (error) =>
      this.failAll(new WorkflowError(`OMP RPC socket error: ${error.message}`, 4)),
    );
    socket.on('close', () =>
      this.failAll(new WorkflowError('OMP RPC socket closed unexpectedly', 4)),
    );
  }

  onData(chunk) {
    this.buffer += chunk;
    while (true) {
      const newline = this.buffer.indexOf('\n');
      if (newline === -1) return;
      const line = this.buffer.slice(0, newline);
      this.buffer = this.buffer.slice(newline + 1);
      if (!line.trim()) continue;
      let frame;
      try {
        frame = JSON.parse(line);
      } catch {
        this.failAll(new WorkflowError(`OMP RPC emitted invalid JSON: ${line}`, 4));
        continue;
      }
      if (frame.type === 'bridge_error') {
        this.failAll(new WorkflowError(`OMP RPC bridge failed: ${frame.message}`, 4));
        continue;
      }
      if (frame.type === 'bridge_exit') {
        this.failAll(new WorkflowError(`OMP RPC process exited with code ${frame.code}`, 4));
        continue;
      }
      if (frame.type === 'extension_ui_request' && typeof frame.id === 'string') {
        this.send({ type: 'extension_ui_response', id: frame.id, cancelled: true });
      }
      this.history.push(frame);
      if (this.history.length > 500) this.history.shift();
      for (const waiter of this.waiters) {
        if (waiter.predicate(frame)) waiter.resolve(frame);
      }
    }
  }

  waitFor(predicate, timeoutMilliseconds, label) {
    const existing = this.history.findLast(predicate);
    if (existing) return Promise.resolve(existing);
    return new Promise((resolve, reject) => {
      const waiter = {
        predicate,
        resolve: (frame) => {
          clearTimeout(timer);
          this.waiters.delete(waiter);
          resolve(frame);
        },
        reject: (error) => {
          clearTimeout(timer);
          this.waiters.delete(waiter);
          reject(error);
        },
      };
      const timer = setTimeout(
        () => waiter.reject(new WorkflowError(`Timed out waiting for OMP RPC ${label}`, 5)),
        timeoutMilliseconds,
      );
      this.waiters.add(waiter);
    });
  }

  failAll(error) {
    for (const waiter of this.waiters) waiter.reject(error);
  }

  send(frame) {
    this.socket.write(`${JSON.stringify(frame)}\n`);
  }

  close() {
    this.socket.end();
  }
}

async function connectRpcChannel(socketPath, timeoutMilliseconds = 10_000) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      const socket = await new Promise((resolve, reject) => {
        const candidate = createConnection(socketPath);
        candidate.once('connect', () => resolve(candidate));
        candidate.once('error', reject);
      });
      return new RpcChannel(socket);
    } catch (error) {
      if (error?.code !== 'ENOENT' && error?.code !== 'ECONNREFUSED') throw error;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }
  throw new WorkflowError('Timed out connecting to the OMP RPC bridge', 5);
}

async function rpcRequest(channel, command, timeoutMilliseconds) {
  const id = `rpc:${command.type}:${randomBytes(8).toString('hex')}`;
  const response = channel.waitFor(
    (frame) => frame.type === 'response' && frame.id === id,
    timeoutMilliseconds,
    `${command.type} response`,
  );
  channel.send({ id, ...command });
  const frame = await response;
  if (!frame.success) throw new WorkflowError(`OMP RPC ${command.type} failed: ${frame.error}`, 4);
  return frame.data;
}

async function runOmpRpc(socketPath, prompt, timeoutMilliseconds) {
  const channel = await connectRpcChannel(socketPath);
  try {
    await channel.waitFor((frame) => frame.type === 'ready', 30_000, 'ready frame');
    const agentEnd = channel.waitFor(
      (frame) => frame.type === 'agent_end',
      timeoutMilliseconds,
      'agent_end event',
    );
    const promptResult = await rpcRequest(channel, { type: 'prompt', message: prompt }, 30_000);
    if (promptResult?.agentInvoked === false) {
      throw new WorkflowError('OMP RPC prompt did not invoke the agent', 4);
    }
    await agentEnd;
    const lastMessage = await rpcRequest(channel, { type: 'get_last_assistant_text' }, 10_000);
    if (typeof lastMessage?.text !== 'string' || lastMessage.text.trim() === '') {
      throw new WorkflowError('OMP RPC returned no final assistant report', 4);
    }
    return lastMessage.text;
  } catch (error) {
    await rpcRequest(channel, { type: 'abort' }, 5000).catch(() => {});
    throw error;
  } finally {
    channel.close();
  }
}

async function acquireEditLock(repoRoot, packet) {
  if (packet.mode !== 'edit') return undefined;

  const lockRoot = path.join(tmpdir(), 'delegate-to-omp-locks');
  const lockPath = path.join(lockRoot, sha256(repoRoot).slice(0, 20));
  const ownerPath = path.join(lockPath, 'owner.json');
  const token = randomBytes(12).toString('hex');
  await mkdir(lockRoot, { recursive: true });

  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      await mkdir(lockPath);
      await writeFile(
        ownerPath,
        JSON.stringify({ pid: process.pid, token, task_id: packet.task_id }, null, 2),
        { mode: 0o600 },
      );
      return { lockPath, ownerPath, token };
    } catch (error) {
      if (error?.code !== 'EEXIST') throw error;
      let owner;
      try {
        owner = JSON.parse(await readFile(ownerPath, 'utf8'));
      } catch {
        owner = undefined;
      }
      let ownerAlive = false;
      if (Number.isInteger(owner?.pid)) {
        try {
          process.kill(owner.pid, 0);
          ownerAlive = true;
        } catch (processError) {
          ownerAlive = processError?.code === 'EPERM';
        }
      }
      if (ownerAlive) {
        throw new WorkflowError(
          `Another edit task holds the repository lock: ${owner?.task_id ?? 'unknown task'}`,
          3,
        );
      }
      await rm(lockPath, { recursive: true, force: true });
    }
  }
  throw new WorkflowError('Could not acquire the repository edit lock', 3);
}

async function releaseEditLock(lock) {
  if (!lock) return;
  try {
    const owner = JSON.parse(await readFile(lock.ownerPath, 'utf8'));
    if (owner.token === lock.token) await rm(lock.lockPath, { recursive: true, force: true });
  } catch {
    // A missing or replaced lock is not ours to remove.
  }
}

async function execute(packetPath, keepPane) {
  if (process.env.HERDR_ENV !== '1') {
    throw new WorkflowError(
      'delegate-to-omp must run inside a Herdr-managed pane (HERDR_ENV=1)',
      2,
    );
  }

  const packet = validatePacket(JSON.parse(await readFile(packetPath, 'utf8')));
  const repoResult = await runCommand('git', ['rev-parse', '--show-toplevel'], {
    cwd: process.cwd(),
  });
  const repoRoot = repoResult.stdout.trim();
  const herdrSocketPath = process.env.HERDR_SOCKET_PATH;
  const callerPaneId = process.env.HERDR_PANE_ID;
  if (!herdrSocketPath || !callerPaneId) {
    throw new WorkflowError(
      'HERDR_SOCKET_PATH and HERDR_PANE_ID are required for Herdr socket delegation',
      2,
    );
  }
  const ompBin = process.env.OMP_BIN || 'omp';
  const nonce = randomBytes(16).toString('hex');
  const runDirectory = await mkdtemp(path.join(tmpdir(), `delegate-to-omp-${packet.task_id}-`));
  const promptPath = path.join(runDirectory, 'prompt.md');
  const reportPath = path.join(runDirectory, 'report.json');
  let paneId;
  let lock;

  try {
    lock = await acquireEditLock(repoRoot, packet);
    const before = await snapshotRepository(repoRoot);
    const prompt = buildPrompt(packet, before, nonce);
    await writeFile(promptPath, prompt.text, { mode: 0o600 });
    const bridge = await createOmpBridge(runDirectory, ompBin, buildOmpArgs(packet, repoRoot));
    const workspaceId = await prepareHerdr(herdrSocketPath, callerPaneId);
    paneId = await startOmpAgent(
      herdrSocketPath,
      packet,
      repoRoot,
      workspaceId,
      bridge.bridgePath,
      bridge.manifestPath,
      nonce,
    );
    await reportHerdrAgent(
      herdrSocketPath,
      paneId,
      'working',
      `Executing ${packet.task_id} through OMP RPC`,
    );

    const assistantText = await runOmpRpc(
      bridge.rpcSocketPath,
      prompt.text,
      packet.timeout_seconds * 1000,
    );
    const report = validateReport(
      extractReport(assistantText, prompt.beginMarker, prompt.endMarker),
      packet,
    );
    const after = await snapshotRepository(repoRoot);
    validateRepositoryDelta(packet, before, after, report.changed_files);
    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600 });
    process.stdout.write(`${renderReport(report, paneId, reportPath)}\n`);

    if (report.status !== 'completed') {
      throw new WorkflowError(
        `OMP reported task status ${report.status}; pane retained: ${paneId}`,
        4,
      );
    }
    await reportHerdrAgent(herdrSocketPath, paneId, 'idle', `Completed ${packet.task_id}`);
    if (!keepPane) {
      await herdrRequest(herdrSocketPath, 'pane.close', { pane_id: paneId });
    }
  } catch (error) {
    if (paneId) {
      await reportHerdrAgent(
        herdrSocketPath,
        paneId,
        'blocked',
        String(error.message).slice(0, 500),
      ).catch(() => {});
    }
    if (paneId && !String(error.message).includes(paneId)) {
      error.message = `${error.message}\nOMP pane retained for inspection: ${paneId}`;
    }
    throw error;
  } finally {
    await releaseEditLock(lock);
  }
}

async function main() {
  try {
    const { packetPath, keepPane } = parseCliArgs(process.argv.slice(2));
    await execute(packetPath, keepPane);
  } catch (error) {
    const exitCode = error instanceof WorkflowError ? error.exitCode : 1;
    process.stderr.write(`delegate-to-omp: ${error.message}\n`);
    process.exitCode = exitCode;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
