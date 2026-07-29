import { commands as aliasCommands } from '@bindings';
import { expect, rstest, test } from '@rstest/core';
import { Effect, Exit } from 'effect';

import { fetchMpvTrackList, fetchNowPlayingState, parseTrackList } from '../src/effects/nowPlaying';

test('parseTrackList parses valid audio and subtitle tracks', () => {
  const tracksJson = JSON.stringify([
    { id: 1, type: 'audio', title: 'English Title', selected: true },
    { id: 2, type: 'sub', lang: 'fre', selected: false },
    { id: 3, type: 'video', codec: 'h264', selected: false }, // Should be omitted
    { id: 4, type: 'audio', codec: 'aac', selected: false },
  ]);

  const result = parseTrackList(tracksJson);
  expect(result).toEqual([
    { id: 1, type: 'audio', label: 'English Title', selected: true },
    { id: 2, type: 'sub', label: 'fre', selected: false },
    { id: 4, type: 'audio', label: 'aac', selected: false },
  ]);
});

test('parseTrackList handles empty and invalid inputs', () => {
  expect(parseTrackList('Null')).toEqual([]);
  expect(parseTrackList('')).toEqual([]);
  expect(parseTrackList('invalid json')).toEqual([]);
  expect(parseTrackList('{}')).toEqual([]); // Non-array JSON
});

test('fetchNowPlayingState runs successfully when mocked', async () => {
  const spy = rstest.spyOn(aliasCommands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: {
      canPlayNext: true,
      canPlayPrevious: true,
      media: null,
      nextUnavailableReason: null,
      player: {
        connected: true,
        duration: 100,
        muted: false,
        paused: false,
        timePos: 50,
        volume: 50,
      },
      previousUnavailableReason: null,
      status: 'playing',
    },
  });

  const exit = await Effect.runPromiseExit(fetchNowPlayingState);
  expect(Exit.isSuccess(exit)).toBe(true);
  if (Exit.isSuccess(exit)) {
    expect(exit.value.status).toBe('playing');
  }
  spy.mockRestore();
});

test('fetchMpvTrackList runs successfully when mocked', async () => {
  const trackList = JSON.stringify([
    { id: 1, lang: 'eng', selected: true, title: 'English Stereo', type: 'audio' },
  ]);
  const spy = rstest.spyOn(aliasCommands, 'mpvGetProperty').mockResolvedValue({
    status: 'ok',
    data: trackList,
  });

  const exit = await Effect.runPromiseExit(fetchMpvTrackList(true));
  expect(Exit.isSuccess(exit)).toBe(true);
  if (Exit.isSuccess(exit)) {
    expect(exit.value).toEqual([{ id: 1, type: 'audio', label: 'English Stereo', selected: true }]);
  }
  spy.mockRestore();
});
