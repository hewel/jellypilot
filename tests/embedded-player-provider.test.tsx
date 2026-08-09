// @rstest-environment jsdom
import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { Show } from 'solid-js';
import { render } from 'solid-js/web';

import { commands, events } from '../src/bindings';
import type { EmbeddedPlayerState } from '../src/bindings';
import {
  EmbeddedPlayerProvider,
  useEmbeddedPlayer,
} from '../src/components/EmbeddedPlayerProvider';

const idleState: EmbeddedPlayerState = {
  canPlayInMpv: false,
  desiredMuted: false,
  desiredPaused: true,
  desiredSeekPositionSeconds: null,
  desiredVolume: 100,
  durationSeconds: null,
  dynamicRange: null,
  failure: null,
  generation: null,
  itemId: null,
  phase: 'idle',
  playlistUrl: null,
  positionSeconds: 0,
  revision: 0,
  sessionId: null,
  subtitle: null,
  timelineOffsetSeconds: 0,
  title: null,
  videoCodec: null,
};

const activeState: EmbeddedPlayerState = {
  ...idleState,
  canPlayInMpv: true,
  desiredPaused: false,
  durationSeconds: 300,
  generation: 7,
  itemId: 'episode-1',
  phase: 'playing',
  playlistUrl: 'http://127.0.0.1:3210/hls/nonce/master.m3u8',
  positionSeconds: 42,
  revision: 1,
  sessionId: 'session-1',
  subtitle: 'Example Show · S01E01',
  title: 'Pilot',
};

afterEach(() => {
  document.body.replaceChildren();
  rstest.restoreAllMocks();
});

function PlayerProbe() {
  const embedded = useEmbeddedPlayer();
  return (
    <div>
      <Show when={embedded.player()} fallback={<span>No player</span>}>
        {(player) => <span>{player().title}</span>}
      </Show>
      <button type="button" onClick={() => embedded.control({ kind: 'restart' })}>
        Restart
      </button>
      <button
        type="button"
        onClick={() =>
          embedded.observe({
            durationSeconds: 300,
            generation: 7,
            kind: 'playing',
            mediaTimeSeconds: 4,
            muted: false,
            seekableEndSeconds: 60,
            seekableStartSeconds: 0,
            sessionId: 'session-1',
            volume: 75,
          })
        }
      >
        Observe
      </button>
    </div>
  );
}

test('provider owns pushed player state and preserves ordered typed command boundaries', async () => {
  rstest.spyOn(commands, 'embeddedPlayerGetState').mockResolvedValue({
    data: idleState,
    status: 'ok',
  });
  rstest.spyOn(commands, 'embeddedPlayerRegisterCapabilities').mockResolvedValue({
    data: idleState,
    status: 'ok',
  });
  const controlResult =
    Promise.withResolvers<Awaited<ReturnType<typeof commands.embeddedPlayerControl>>>();
  const control = rstest
    .spyOn(commands, 'embeddedPlayerControl')
    .mockImplementation(() => controlResult.promise);
  const observe = rstest.spyOn(commands, 'embeddedPlayerObserve').mockResolvedValue({
    data: activeState,
    status: 'ok',
  });
  let pushState: ((state: EmbeddedPlayerState) => void) | undefined;
  rstest.spyOn(events.embeddedPlayerChanged, 'listen').mockImplementation((handler) => {
    pushState = (state) => handler({ event: 'embedded-player-changed', id: 0, payload: { state } });
    return Promise.resolve(() => {});
  });
  const onActivePlayerChanged = rstest.fn();
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <EmbeddedPlayerProvider onActivePlayerChanged={onActivePlayerChanged}>
        <PlayerProbe />
      </EmbeddedPlayerProvider>
    ),
    root,
  );

  await screen.findByText('No player');
  pushState?.(activeState);
  await screen.findByText('Pilot');
  fireEvent.click(screen.getByRole('button', { name: 'Restart' }));
  pushState?.({ ...activeState, revision: 2, title: 'New Pilot' });
  controlResult.resolve({ data: activeState, status: 'ok' });
  fireEvent.click(screen.getByRole('button', { name: 'Observe' }));

  await waitFor(() => expect(control).toHaveBeenCalledWith({ kind: 'restart' }));
  await waitFor(() =>
    expect(observe).toHaveBeenCalledWith(
      expect.objectContaining({
        generation: 7,
        mediaTimeSeconds: 4,
        sequence: 1,
        sessionId: 'session-1',
      }),
    ),
  );
  await screen.findByText('New Pilot');
  expect(screen.queryByText('Pilot')).toBeNull();
  expect(onActivePlayerChanged).toHaveBeenCalledTimes(1);
  dispose();
});
