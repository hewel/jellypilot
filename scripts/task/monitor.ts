import {
  access,
  appendFile,
  constants,
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
} from 'node:fs/promises';
import { dirname, join, resolve, sep } from 'node:path';

import { Effect } from 'effect';

import { REPO_ROOT } from './constants';
import { TaskMonitorError } from './errors';

const BYTES_PER_KIBIBYTE = 1024;
const BYTES_PER_MEBIBYTE = BYTES_PER_KIBIBYTE * BYTES_PER_KIBIBYTE;
export const MONITOR_DEFAULT_SAMPLES = 301;
export const MONITOR_DEFAULT_INTERVAL_MS = 1000;

export interface MonitorTask {
  readonly pid: number;
  readonly samples: number;
  readonly intervalMs: number;
  readonly output: string;
  readonly label: string | null;
}

interface ClockTicks {
  readonly hertz: number;
}

interface ProcSnapshot {
  readonly name: string;
  readonly utimeTicks: number;
  readonly stimeTicks: number;
  readonly startTimeTicks: number;
}

interface MemoryState {
  readonly available: boolean;
  readonly rssKiB: number | null;
  readonly pssKiB: number | null;
}

interface GpuMetricState {
  readonly available: boolean;
  readonly residentBytes: number | null;
  readonly engineNanoseconds: number | null;
}

interface ProcessObservation {
  readonly elapsedMilliseconds: number;
  readonly name: string;
  readonly startTimeTicks: number;
  readonly memory: MemoryState;
  readonly cpuTimeMilliseconds: number;
  readonly contextSwitches: number;
  readonly gpu: GpuMetricState;
}

interface NumericSummary {
  readonly mean: number;
  readonly median: number;
  readonly p95: number;
  readonly max: number;
}

interface ObservationSummary {
  readonly memory: {
    readonly available: boolean;
    readonly rssKiB: NumericSummary | null;
    readonly pssKiB: NumericSummary | null;
  };
  readonly cpu: {
    readonly timeMillisecondsDelta: number;
    readonly timeMillisecondsPerSecond: number;
    readonly contextSwitchesPerSecond: number;
  };
  readonly gpu: {
    readonly residentBytes: NumericSummary | null;
    readonly engineNanosecondsPerSecond: number | null;
  };
}

export function percentile(sortedValues: readonly number[], fraction: number): number {
  if (sortedValues.length === 0) return 0;
  const index = Math.min(
    sortedValues.length - 1,
    Math.max(0, Math.ceil(fraction * sortedValues.length) - 1),
  );
  return sortedValues[index] ?? 0;
}

export function summarizeObservations(
  observations: readonly ProcessObservation[],
): ObservationSummary {
  if (observations.length === 0) {
    return {
      memory: { available: false, rssKiB: null, pssKiB: null },
      cpu: { timeMillisecondsDelta: 0, timeMillisecondsPerSecond: 0, contextSwitchesPerSecond: 0 },
      gpu: { residentBytes: null, engineNanosecondsPerSecond: null },
    };
  }

  const first = observations[0];
  const last = observations[observations.length - 1];
  if (first === undefined || last === undefined) {
    throw new Error('Non-empty observations had no boundary sample.');
  }

  const elapsedSeconds = Math.max(last.elapsedMilliseconds - first.elapsedMilliseconds, 0) / 1000;
  const rssValues = observations.flatMap((observation) =>
    observation.memory.available && observation.memory.rssKiB !== null
      ? [observation.memory.rssKiB]
      : [],
  );
  const pssValues = observations.flatMap((observation) =>
    observation.memory.available && observation.memory.pssKiB !== null
      ? [observation.memory.pssKiB]
      : [],
  );
  const gpuResidentValues = observations.flatMap((observation) =>
    observation.gpu.available && observation.gpu.residentBytes !== null
      ? [observation.gpu.residentBytes]
      : [],
  );
  const cpuDelta = last.cpuTimeMilliseconds - first.cpuTimeMilliseconds;
  const contextSwitchDelta = last.contextSwitches - first.contextSwitches;
  const engineDelta =
    first.gpu.available &&
    first.gpu.engineNanoseconds !== null &&
    last.gpu.engineNanoseconds !== null
      ? last.gpu.engineNanoseconds - first.gpu.engineNanoseconds
      : null;

  return {
    memory:
      rssValues.length > 0 || pssValues.length > 0
        ? {
            available: true,
            rssKiB: numericSummary(rssValues),
            pssKiB: numericSummary(pssValues),
          }
        : { available: false, rssKiB: null, pssKiB: null },
    cpu: {
      timeMillisecondsDelta: cpuDelta,
      timeMillisecondsPerSecond: elapsedSeconds > 0 ? cpuDelta / elapsedSeconds : 0,
      contextSwitchesPerSecond: elapsedSeconds > 0 ? contextSwitchDelta / elapsedSeconds : 0,
    },
    gpu: {
      residentBytes: numericSummary(gpuResidentValues),
      engineNanosecondsPerSecond:
        engineDelta === null
          ? null
          : engineDelta < 0 || elapsedSeconds <= 0
            ? null
            : engineDelta / elapsedSeconds,
    },
  };
}

export const runMonitor = Effect.fn('task.monitor')((task: MonitorTask) =>
  Effect.gen(function* () {
    const clock = yield* readClockTicks();
    const startedAt = performance.timeOrigin + performance.now();
    const wallClockStartedAt = new Date().toISOString();
    const samples: ProcessObservation[] = [
      yield* observeProcess(
        task.pid,
        performance.timeOrigin + performance.now() - startedAt,
        clock,
      ),
    ];
    const expectedStartTime = samples[0]?.startTimeTicks;
    if (expectedStartTime === undefined) {
      return yield* Effect.fail(
        new TaskMonitorError({
          pid: task.pid,
          message: 'The target process exited before sampling.',
        }),
      );
    }

    for (let sampleIndex = 1; sampleIndex < task.samples; sampleIndex += 1) {
      const targetTime = sampleIndex * task.intervalMs;
      const delay = targetTime - (performance.timeOrigin + performance.now() - startedAt);
      if (delay > 0) yield* Effect.sleep(delay);
      const sample = yield* observeProcess(
        task.pid,
        performance.timeOrigin + performance.now() - startedAt,
        clock,
      );
      if (sample.startTimeTicks !== expectedStartTime) {
        return yield* Effect.fail(
          new TaskMonitorError({
            pid: task.pid,
            message: `The target process identity changed from start time ${expectedStartTime} to ${sample.startTimeTicks}.`,
          }),
        );
      }
      samples.push(sample);
    }

    const report = {
      schema: 1,
      platform: 'linux',
      protocol: {
        kind: 'raw sampler',
        stabilization:
          'external; the caller completes the selected five-minute state before starting this command',
        observation: `samples t=0 through t=${(task.samples - 1) * task.intervalMs} ms`,
      },
      pid: task.pid,
      label: task.label,
      processStartTimeTicks: expectedStartTime,
      samples: samples.length,
      intervalMilliseconds: task.intervalMs,
      startedAt: wallClockStartedAt,
      first: samples[0],
      last: samples[samples.length - 1],
      summary: summarizeObservations(samples),
    };

    const outputPath = yield* resolveOutputPath(task.output);
    yield* Effect.tryPromise({
      try: async () => {
        await mkdir(dirname(outputPath), { recursive: true });
        await appendFile(outputPath, `${JSON.stringify(report)}\n`);
      },
      catch: (cause) =>
        new TaskMonitorError({
          pid: task.pid,
          message: `Could not write monitor output: ${errorMessage(cause)}`,
        }),
    });
    yield* Effect.sync(() => console.log(JSON.stringify(report)));
  }),
);

const readClockTicks = Effect.fn('task.monitor.clock')(() =>
  Effect.tryPromise({
    try: async () => {
      const output = Bun.spawnSync({
        cmd: ['getconf', 'CLK_TCK'],
        stdout: 'pipe',
        stderr: 'pipe',
      });
      const hertz = Number(output.stdout.toString().trim());
      if (!Number.isFinite(hertz) || hertz <= 0) {
        throw new Error('Could not read the clock tick frequency.');
      }
      return { hertz } satisfies ClockTicks;
    },
    catch: (cause) =>
      new TaskMonitorError({
        pid: 0,
        message: `Could not initialize clocks: ${errorMessage(cause)}`,
      }),
  }),
);

const observeProcess = Effect.fn('task.monitor.observe')(
  (pid: number, elapsedMs: number, clock: ClockTicks) =>
    Effect.gen(function* () {
      const snapshot = yield* readProcess(pid);
      const memory = yield* readMemory(pid);
      const contextSwitches = yield* readProcessContextSwitches(pid);
      const gpu = yield* readGpu(pid);
      return {
        elapsedMilliseconds: elapsedMs,
        name: snapshot.name,
        startTimeTicks: snapshot.startTimeTicks,
        memory,
        cpuTimeMilliseconds: ((snapshot.utimeTicks + snapshot.stimeTicks) * 1000) / clock.hertz,
        contextSwitches,
        gpu,
      } satisfies ProcessObservation;
    }),
);

const readProcess = Effect.fn('task.monitor.process')((pid: number) =>
  Effect.tryPromise({
    try: async () => parseProcStat(await readFile(`/proc/${pid}/stat`, 'utf8'), pid),
    catch: (cause) =>
      new TaskMonitorError({ pid, message: `Could not read /proc/${pid}: ${errorMessage(cause)}` }),
  }),
);

export function parseProcStat(text: string, pid: number): ProcSnapshot {
  const closeParen = text.lastIndexOf(')');
  const openParen = text.indexOf('(');
  if (openParen === -1 || closeParen === -1 || closeParen < openParen) {
    throw new Error(`/proc/${pid}/stat has an unexpected shape.`);
  }
  const name = text.slice(openParen + 1, closeParen);
  const fields = text
    .slice(closeParen + 2)
    .split(/\s+/)
    .filter((field) => field.length > 0);
  const field = (index: number): number => {
    const value = Number(fields[index]);
    if (!Number.isFinite(value)) throw new Error(`/proc/${pid}/stat has an invalid field.`);
    return value;
  };
  return {
    name,
    utimeTicks: field(11),
    stimeTicks: field(12),
    startTimeTicks: field(19),
  };
}

const readMemory = Effect.fn('task.monitor.memory')((pid: number) =>
  Effect.tryPromise({
    try: async () => {
      const text = await readFile(`/proc/${pid}/smaps_rollup`, 'utf8');
      const rssKiB = Number(/^Rss:\s+(\d+)\s+kB/m.exec(text)?.[1]);
      const pssKiB = Number(/^Pss:\s+(\d+)\s+kB/m.exec(text)?.[1]);
      if (!Number.isFinite(rssKiB) || !Number.isFinite(pssKiB)) {
        throw new TypeError('smaps_rollup is missing Rss or Pss.');
      }
      return { available: true, rssKiB, pssKiB } satisfies MemoryState;
    },
    catch: (cause) =>
      new TaskMonitorError({
        pid,
        message: `Could not read process memory: ${errorMessage(cause)}`,
      }),
  }).pipe(
    Effect.catchTag('TaskMonitorError', (error) => {
      if (!isMissingFileMessage(error.message) && !error.message.includes('EACCES')) {
        return Effect.fail(error);
      }
      return Effect.succeed({ available: false, rssKiB: null, pssKiB: null });
    }),
  ),
);

const readProcessContextSwitches = Effect.fn('task.monitor.context-switches')((pid: number) =>
  Effect.tryPromise({
    try: async () => {
      const taskDirectory = `/proc/${pid}/task`;
      const threadIds = await readdir(taskDirectory);
      let contextSwitches = 0;
      for (const threadId of threadIds) {
        const text = await readFile(join(taskDirectory, threadId, 'status'), 'utf8');
        contextSwitches +=
          parseStatusCounter(text, pid, threadId, 'voluntary_ctxt_switches') +
          parseStatusCounter(text, pid, threadId, 'nonvoluntary_ctxt_switches');
      }
      return contextSwitches;
    },
    catch: (cause) =>
      new TaskMonitorError({
        pid,
        message: `Could not read process thread context switches: ${errorMessage(cause)}`,
      }),
  }),
);

function parseStatusCounter(text: string, pid: number, threadId: string, name: string): number {
  const value = Number(new RegExp(`^${name}:\\s+(\\d+)`, 'm').exec(text)?.[1]);
  if (!Number.isFinite(value)) {
    throw new TypeError(`/proc/${pid}/task/${threadId}/status is missing ${name}.`);
  }
  return value;
}

const readGpu = Effect.fn('task.monitor.gpu')((pid: number) =>
  Effect.tryPromise({
    try: async () => {
      const fdinfoDirectory = `/proc/${pid}/fdinfo`;
      const fdinfoNames = await readdir(fdinfoDirectory);
      const seenClients = new Set<string>();
      let residentBytes = 0;
      let engineNanoseconds = 0;
      let hasResident = false;
      let hasEngine = false;

      for (const name of fdinfoNames) {
        const text = await readFile(join(fdinfoDirectory, name), 'utf8').catch((error) => {
          if (isMissingFile(error)) return '';
          throw error;
        });
        if (!text.includes('drm-driver:')) continue;
        const pdev = /^drm-pdev:\s*(\S+)\s*$/m.exec(text)?.[1];
        const clientId = /^drm-client-id:\s*(\d+)\s*$/m.exec(text)?.[1];
        if (clientId !== undefined) {
          const identity = `${pdev ?? 'unknown'}:${clientId}`;
          if (seenClients.has(identity)) continue;
          seenClients.add(identity);
        }

        let hasResidentValue = false;
        let resident = 0;
        for (const match of text.matchAll(/^drm-resident-\S+:\s+(\d+)\s*(KiB|MiB)?\s*$/gm)) {
          const value = Number(match[1]);
          resident +=
            match[2] === 'MiB'
              ? value * BYTES_PER_MEBIBYTE
              : match[2] === 'KiB'
                ? value * BYTES_PER_KIBIBYTE
                : value;
          hasResidentValue = true;
        }
        let hasEngineValue = false;
        let engine = 0;
        for (const match of text.matchAll(/^drm-engine-\S+:\s+(\d+)\s+ns\s*$/gm)) {
          engine += Number(match[1]);
          hasEngineValue = true;
        }
        residentBytes += resident;
        engineNanoseconds += engine;
        hasResident ||= hasResidentValue;
        hasEngine ||= hasEngineValue;
      }

      return hasResident || hasEngine
        ? {
            available: true,
            residentBytes: hasResident ? residentBytes : null,
            engineNanoseconds: hasEngine ? engineNanoseconds : null,
          }
        : { available: false, residentBytes: null, engineNanoseconds: null };
    },
    catch: (cause) =>
      new TaskMonitorError({ pid, message: `Could not read DRM fdinfo: ${errorMessage(cause)}` }),
  }).pipe(
    Effect.catchTag('TaskMonitorError', (error) => {
      if (!isMissingFileMessage(error.message)) return Effect.fail(error);
      return Effect.succeed({ available: false, residentBytes: null, engineNanoseconds: null });
    }),
  ),
);

function numericSummary(values: readonly number[]): NumericSummary | null {
  if (values.length === 0) return null;
  const sortedValues = values.toSorted((left, right) => left - right);
  return {
    mean: sortedValues.reduce((sum, value) => sum + value, 0) / sortedValues.length,
    median: percentile(sortedValues, 0.5),
    p95: percentile(sortedValues, 0.95),
    max: sortedValues[sortedValues.length - 1] ?? 0,
  };
}

const resolveOutputPath = Effect.fn('task.monitor.output')((output: string) =>
  Effect.gen(function* () {
    const lexical = resolve(REPO_ROOT, output);
    const targetRoot = join(REPO_ROOT, 'target');
    const targetPrefix = `${targetRoot}${sep}`;
    if (!`${lexical}${sep}`.startsWith(targetPrefix)) {
      return yield* Effect.fail(
        new TaskMonitorError({
          pid: 0,
          message: `Monitor output must stay under ${targetRoot}: ${output}`,
        }),
      );
    }
    const checked = yield* Effect.tryPromise({
      try: async () => {
        await mkdir(targetRoot, { recursive: true });
        const realTargetRoot = await realpath(targetRoot);
        const nearestExisting = await nearestExistingPath(lexical);
        const realExisting = await realpath(nearestExisting);
        if (
          realExisting !== realTargetRoot &&
          !realExisting.startsWith(`${realTargetRoot}${sep}`)
        ) {
          throw new Error(`Output resolves outside ${realTargetRoot}: ${output}`);
        }
        const fileStatus = await lstat(lexical).catch((error) => {
          if (isMissingFile(error)) return null;
          throw error;
        });
        if (fileStatus?.isSymbolicLink())
          throw new Error(`Output file must not be a symlink: ${output}`);
        return lexical;
      },
      catch: (cause) =>
        new TaskMonitorError({
          pid: 0,
          message: `Monitor output is not confined: ${errorMessage(cause)}`,
        }),
    });
    return checked;
  }),
);

async function nearestExistingPath(path: string): Promise<string> {
  let candidate = path;
  for (;;) {
    try {
      await access(candidate, constants.F_OK);
      return candidate;
    } catch (error) {
      if (!isMissingFile(error)) throw error;
      const parent = dirname(candidate);
      if (parent === candidate) throw error;
      candidate = parent;
    }
  }
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function isMissingFile(cause: unknown): boolean {
  return typeof cause === 'object' && cause !== null && 'code' in cause && cause.code === 'ENOENT';
}

function isMissingFileMessage(message: string): boolean {
  return message.includes('ENOENT') || message.includes('no such file');
}
