import { afterEach, expect, rstest, test } from '@rstest/core';
import { Cause, Effect, Exit, Fiber } from 'effect';

import { commands } from '../src/bindings';
import { runQuickConnectWorkflow } from '../src/effects/quickConnect';
import { loadSavedSession } from '../src/sessionAccess';

const sampleSession = {
  accessToken: 'token-123',
  deviceId: 'device-123',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

afterEach(() => {
  rstest.restoreAllMocks();
  rstest.useRealTimers();
  localStorage.clear();
});

test('starts quick connect, calls onCode, and polls every 5 seconds', async () => {
  rstest.useFakeTimers();

  const startSpy = rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  const checkSpy = rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'waiting',
    status: 'ok',
  });

  const onCode = rstest.fn();
  const fiber = Effect.runFork(runQuickConnectWorkflow('https://jellyfin.example.com', onCode));

  // Let the microtasks run to execute the start command
  await rstest.advanceTimersByTimeAsync(0);

  expect(startSpy).toHaveBeenCalledWith('https://jellyfin.example.com');
  expect(onCode).toHaveBeenCalledWith('ABCD12');
  expect(checkSpy).not.toHaveBeenCalled();

  // Advance 5 seconds to trigger first poll
  await rstest.advanceTimersByTimeAsync(5000);
  expect(checkSpy).toHaveBeenCalledTimes(1);

  // Advance another 5 seconds to trigger second poll
  await rstest.advanceTimersByTimeAsync(5000);
  expect(checkSpy).toHaveBeenCalledTimes(2);

  // Interrupt the fiber to clean up
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test('successful approval, authentication, and session save', async () => {
  rstest.useFakeTimers();

  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  const checkMock = rstest
    .spyOn(commands, 'jellyfinQuickConnectCheck')
    .mockResolvedValueOnce({ data: 'waiting', status: 'ok' })
    .mockResolvedValueOnce({ data: 'approved', status: 'ok' });
  const authMock = rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const sessionMock = rstest.spyOn(commands, 'jellyfinGetSession').mockResolvedValue(sampleSession);

  const onCode = rstest.fn();
  const runPromise = Effect.runPromiseExit(
    runQuickConnectWorkflow('https://jellyfin.example.com', onCode),
  );

  // Let start complete
  await rstest.advanceTimersByTimeAsync(0);
  expect(onCode).toHaveBeenCalledWith('ABCD12');

  // Advance 5s (first poll: waiting)
  await rstest.advanceTimersByTimeAsync(5000);
  expect(checkMock).toHaveBeenCalledTimes(1);
  expect(authMock).not.toHaveBeenCalled();

  // Advance another 5s (second poll: approved)
  await rstest.advanceTimersByTimeAsync(5000);
  expect(checkMock).toHaveBeenCalledTimes(2);

  // Let authenticate and session get resolve
  await rstest.advanceTimersByTimeAsync(0);
  expect(authMock).toHaveBeenCalledWith('https://jellyfin.example.com', 'secret-123');
  expect(sessionMock).toHaveBeenCalledTimes(1);

  const exit = await runPromise;
  expect(Exit.isSuccess(exit)).toBe(true);
  expect(loadSavedSession()).toEqual(sampleSession);
});

test('timeout fail after 5 minutes', async () => {
  rstest.useFakeTimers();

  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'waiting',
    status: 'ok',
  });

  const onCode = rstest.fn();
  const runPromise = Effect.runPromiseExit(
    runQuickConnectWorkflow('https://jellyfin.example.com', onCode),
  );

  await rstest.advanceTimersByTimeAsync(0);

  // Advance 5 minutes (5 * 60 * 1000 = 300000ms)
  await rstest.advanceTimersByTimeAsync(300_000);

  const exit = await runPromise;
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const error = exit.cause.reasons[0];
    const message = Cause.isFailReason(error) ? error.error.message : '';
    expect(message).toBe('Quick Connect code expired. Request a new code to try again.');
  }
});

test('cancellation / interruption', async () => {
  rstest.useFakeTimers();

  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  const checkMock = rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'waiting',
    status: 'ok',
  });
  const authMock = rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate');

  const onCode = rstest.fn();
  const fiber = Effect.runFork(runQuickConnectWorkflow('https://jellyfin.example.com', onCode));

  await rstest.advanceTimersByTimeAsync(0);
  await rstest.advanceTimersByTimeAsync(5000);
  expect(checkMock).toHaveBeenCalledTimes(1);

  // Interrupt the fiber
  const interruptPromise = Effect.runPromiseExit(Fiber.interrupt(fiber));
  await rstest.advanceTimersByTimeAsync(0);
  const exit = await interruptPromise;
  expect(Exit.isSuccess(exit)).toBe(true);

  // Advance time and check that no further polls or authenticate calls are made
  await rstest.advanceTimersByTimeAsync(5000);
  expect(checkMock).toHaveBeenCalledTimes(1);
  expect(authMock).not.toHaveBeenCalled();
});

test('start failure propagation', async () => {
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    error: { code: 'network', message: 'Server unavailable' },
    status: 'error',
  });

  const onCode = rstest.fn();
  const exit = await Effect.runPromiseExit(
    runQuickConnectWorkflow('https://jellyfin.example.com', onCode),
  );

  expect(Exit.isFailure(exit)).toBe(true);
  expect(onCode).not.toHaveBeenCalled();
});

test('polling failure propagation', async () => {
  rstest.useFakeTimers();

  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    error: { code: 'network', message: 'Approval polling failed' },
    status: 'error',
  });

  const onCode = rstest.fn();
  const runPromise = Effect.runPromiseExit(
    runQuickConnectWorkflow('https://jellyfin.example.com', onCode),
  );

  await rstest.advanceTimersByTimeAsync(0);
  await rstest.advanceTimersByTimeAsync(5000);

  const exit = await runPromise;
  expect(Exit.isFailure(exit)).toBe(true);
});

test('authentication failure propagation', async () => {
  rstest.useFakeTimers();

  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'approved',
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate').mockResolvedValue({
    error: { code: 'authFailed', message: 'Authentication failed' },
    status: 'error',
  });

  const onCode = rstest.fn();
  const runPromise = Effect.runPromiseExit(
    runQuickConnectWorkflow('https://jellyfin.example.com', onCode),
  );

  await rstest.advanceTimersByTimeAsync(0);
  await rstest.advanceTimersByTimeAsync(5000);

  const exit = await runPromise;
  expect(Exit.isFailure(exit)).toBe(true);
});
