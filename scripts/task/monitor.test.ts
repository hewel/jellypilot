import { describe, expect, test } from 'bun:test';

import { parseProcStat, percentile, summarizeObservations } from './monitor';

describe('monitor summaries', () => {
  test('summarizes process and GPU metrics without inventing unavailable data', () => {
    const summary = summarizeObservations([
      {
        elapsedMilliseconds: 0,
        name: 'jellypilot',
        startTimeTicks: 100,
        memory: { available: true, rssKiB: 100, pssKiB: 60 },
        cpuTimeMilliseconds: 10,
        contextSwitches: 2,
        gpu: { available: true, residentBytes: 1024, engineNanoseconds: 100 },
      },
      {
        elapsedMilliseconds: 1000,
        name: 'jellypilot',
        startTimeTicks: 100,
        memory: { available: true, rssKiB: 120, pssKiB: 70 },
        cpuTimeMilliseconds: 20,
        contextSwitches: 4,
        gpu: { available: true, residentBytes: 2048, engineNanoseconds: 300 },
      },
    ]);

    expect(summary.memory.available).toBe(true);
    expect(summary.memory.rssKiB?.mean).toBe(110);
    expect(summary.memory.pssKiB?.median).toBe(60);
    expect(summary.cpu.timeMillisecondsDelta).toBe(10);
    expect(summary.cpu.timeMillisecondsPerSecond).toBe(10);
    expect(summary.cpu.contextSwitchesPerSecond).toBe(2);
    expect(summary.gpu.residentBytes?.max).toBe(2048);
    expect(summary.gpu.engineNanosecondsPerSecond).toBe(200);
  });

  test('marks memory and GPU metrics unavailable when Linux cannot attribute them', () => {
    const summary = summarizeObservations([
      {
        elapsedMilliseconds: 0,
        name: 'jellypilot',
        startTimeTicks: 100,
        memory: { available: false, rssKiB: null, pssKiB: null },
        cpuTimeMilliseconds: 0,
        contextSwitches: 0,
        gpu: { available: false, residentBytes: null, engineNanoseconds: null },
      },
    ]);

    expect(summary.memory).toEqual({ available: false, rssKiB: null, pssKiB: null });
    expect(summary.gpu).toEqual({ residentBytes: null, engineNanosecondsPerSecond: null });
  });

  test('rejects non-monotonic GPU engine totals instead of reporting a negative rate', () => {
    const summary = summarizeObservations([
      {
        elapsedMilliseconds: 0,
        name: 'jellypilot',
        startTimeTicks: 100,
        memory: { available: true, rssKiB: 100, pssKiB: 60 },
        cpuTimeMilliseconds: 0,
        contextSwitches: 0,
        gpu: { available: true, residentBytes: 100, engineNanoseconds: 200 },
      },
      {
        elapsedMilliseconds: 1000,
        name: 'jellypilot',
        startTimeTicks: 100,
        memory: { available: true, rssKiB: 100, pssKiB: 60 },
        cpuTimeMilliseconds: 0,
        contextSwitches: 0,
        gpu: { available: true, residentBytes: 100, engineNanoseconds: 100 },
      },
    ]);

    expect(summary.gpu.engineNanosecondsPerSecond).toBeNull();
  });

  test('computes nearest-rank percentiles', () => {
    expect(percentile([1, 2, 3, 4], 0.5)).toBe(2);
    expect(percentile([1, 2, 3, 4], 0.95)).toBe(4);
    expect(percentile([], 0.95)).toBe(0);
  });

  test('parses Linux proc stat CPU ticks and process start after a name containing spaces', () => {
    const fields = Array.from({ length: 50 }, (_, index) => String(index + 1));
    fields[11] = '123';
    fields[12] = '45';
    fields[19] = '6789';
    const parsed = parseProcStat(`9999 (comm with spaces) ${fields.join(' ')}`, 9999);

    expect(parsed).toEqual({
      name: 'comm with spaces',
      utimeTicks: 123,
      stimeTicks: 45,
      startTimeTicks: 6789,
    });
  });
});
