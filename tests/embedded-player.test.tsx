// @rstest-environment jsdom
import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import * as HlsModule from 'hls.js';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

import EmbeddedPlayer from '../src/components/EmbeddedPlayer';
import type {
  EmbeddedPlayerControl,
  EmbeddedPlayerObservation,
  EmbeddedPlayerViewModel,
} from '../src/components/EmbeddedPlayer';

const playingModel: EmbeddedPlayerViewModel = {
  canPlayInMpv: true,
  desiredMuted: false,
  desiredPaused: true,
  desiredSeekPositionSeconds: null,
  desiredVolume: 80,
  durationSeconds: 300,
  failureMessage: null,
  generation: 7,
  phase: 'paused',
  positionSeconds: 75,
  sessionId: 'session-1',
  media: { kind: 'hls', url: 'http://127.0.0.1:3210/hls/session/master.m3u8' },
  subtitle: 'Example Show · S01E02',
  timelineOffsetSeconds: 60,
  title: 'The Episode',
};

afterEach(() => {
  document.body.replaceChildren();
  rstest.restoreAllMocks();
});

function renderPlayer(
  model: EmbeddedPlayerViewModel = playingModel,
  onControl = rstest.fn<(command: EmbeddedPlayerControl) => void>(),
  onExit = rstest.fn<() => void>(),
  onPlayInMpv = rstest.fn<() => void>(),
  onObservation = rstest.fn<(observation: EmbeddedPlayerObservation) => void>(),
) {
  const [currentModel, setCurrentModel] = createSignal(model);
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <EmbeddedPlayer
        player={currentModel}
        effects={{
          onControl,
          onExit,
          onObservation,
          onPlayInMpv,
        }}
      />
    ),
    root,
  );
  return { dispose, onControl, onExit, onObservation, onPlayInMpv, setCurrentModel };
}

test('seek reports the absolute library position once when HLS starts from an offset', async () => {
  const { dispose, onControl, onObservation } = renderPlayer();
  const video = document.querySelector('video')!;
  await Promise.resolve();
  Object.defineProperty(video, 'duration', { configurable: true, value: 240 });
  fireEvent.loadedMetadata(video);

  const seek = screen.getByLabelText('Seek position');
  await waitFor(() => expect(seek).toBeEnabled());
  fireEvent.input(seek, { target: { value: '90' } });

  expect(video.currentTime).toBe(30);
  expect(onControl).toHaveBeenCalledWith({ kind: 'seek', positionSeconds: 90 });
  expect(onObservation).toHaveBeenCalledWith(
    expect.objectContaining({ generation: 7, sessionId: 'session-1' }),
  );
  dispose();
});

test('direct media assigns the video source without instantiating HLS', () => {
  const supported = rstest.spyOn(HlsModule.default, 'isSupported').mockReturnValue(true);
  const attachMedia = rstest.spyOn(HlsModule.default.prototype, 'attachMedia');
  const load = rstest.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => undefined);
  const { dispose } = renderPlayer({
    ...playingModel,
    media: {
      kind: 'directSource',
      mimeType: 'video/mp4',
      url: 'http://127.0.0.1:3210/direct/session/video.mp4',
    },
  });

  const video = document.querySelector('video')!;

  expect(video.src).toBe('http://127.0.0.1:3210/direct/session/video.mp4');
  expect(load).toHaveBeenCalled();
  expect(supported).not.toHaveBeenCalled();
  expect(attachMedia).not.toHaveBeenCalled();
  dispose();
});

test('HLS media keeps the hls.js source and media attachment flow', () => {
  const supported = rstest.spyOn(HlsModule.default, 'isSupported').mockReturnValue(true);
  const loadSource = rstest.spyOn(HlsModule.default.prototype, 'loadSource');
  const attachMedia = rstest.spyOn(HlsModule.default.prototype, 'attachMedia');
  const { dispose } = renderPlayer();

  expect(supported).toHaveBeenCalled();
  expect(loadSource).toHaveBeenCalledWith('http://127.0.0.1:3210/hls/session/master.m3u8');
  expect(attachMedia).toHaveBeenCalledWith(document.querySelector('video'));
  dispose();
});

test('switching media destroys the retired HLS session and retains its observation identity', async () => {
  const supported = rstest.spyOn(HlsModule.default, 'isSupported').mockReturnValue(true);
  const destroy = rstest.spyOn(HlsModule.default.prototype, 'destroy');
  const { dispose, onObservation, setCurrentModel } = renderPlayer();
  const retiredVideo = document.querySelector('video')!;
  await Promise.resolve();

  setCurrentModel({
    ...playingModel,
    generation: 8,
    sessionId: 'session-2',
    media: {
      kind: 'directSource',
      mimeType: 'video/mp4',
      url: 'http://127.0.0.1:3210/direct/session-2/video.mp4',
    },
  });
  await waitFor(() => expect(document.querySelector('video')).not.toBe(retiredVideo));

  fireEvent.timeUpdate(retiredVideo);

  expect(supported).toHaveBeenCalledTimes(1);
  expect(destroy).toHaveBeenCalledTimes(1);
  expect(onObservation).toHaveBeenLastCalledWith(
    expect.objectContaining({
      generation: 7,
      kind: 'paused',
      sessionId: 'session-1',
    }),
  );
  dispose();
});

test('failed playback retries through the Rust core and exposes explicit MPV fallback', () => {
  const { dispose, onControl, onPlayInMpv } = renderPlayer({
    ...playingModel,
    failureMessage: 'FFmpeg stopped unexpectedly',
    phase: 'failed',
    media: null,
  });
  const mpvButton = screen.getByRole('button', { name: 'Play in MPV' });

  fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
  fireEvent.click(mpvButton);

  expect(onControl).toHaveBeenCalledWith({ kind: 'restart' });
  expect(onPlayInMpv).toHaveBeenCalledTimes(1);
  expect(mpvButton).toBeEnabled();
  dispose();
});

test('close control stops the active core session and exits the immersive route', () => {
  const { dispose, onControl, onExit } = renderPlayer();

  fireEvent.click(screen.getByRole('button', { name: 'Stop playback and close player' }));

  expect(onControl).toHaveBeenCalledWith({ kind: 'stop' });
  expect(onExit).toHaveBeenCalledTimes(1);
  dispose();
});
