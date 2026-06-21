import * as fs from 'fs';
import * as path from 'path';

import { expect, rstest, test } from '@rstest/core';

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

test('source-boundary assertions for Now Playing files', () => {
  const cardPath = path.resolve(__dirname, '../src/components/NowPlayingCard.tsx');
  const drawerPath = path.resolve(__dirname, '../src/components/NowPlayingDrawer.tsx');
  const effectsPath = path.resolve(__dirname, '../src/effects/nowPlaying.ts');

  const cardContent = fs.readFileSync(cardPath, 'utf8');
  const drawerContent = fs.readFileSync(drawerPath, 'utf8');
  const effectsContent = fs.readFileSync(effectsPath, 'utf8');

  // Verify NowPlayingCard.tsx does not import commands or events directly
  expect(cardContent).not.toContain('commands,');
  expect(cardContent).not.toContain(', commands');
  expect(cardContent).not.toContain('events,');
  expect(cardContent).not.toContain(', events');
  expect(cardContent).not.toContain('commands.');
  expect(cardContent).not.toContain('events.');

  // Verify NowPlayingDrawer.tsx does not import commands or events directly
  expect(drawerContent).not.toContain('commands,');
  expect(drawerContent).not.toContain(', commands');
  expect(drawerContent).not.toContain('events,');
  expect(drawerContent).not.toContain(', events');
  expect(drawerContent).not.toContain('commands.');
  expect(drawerContent).not.toContain('events.');

  // Verify src/effects/nowPlaying.ts contains expected command/event usages
  expect(effectsContent).toContain('commands.nowPlayingGetState');
  expect(effectsContent).toContain("commands.mpvGetProperty('track-list')");
  expect(effectsContent).toContain('commands.mpvSetAudioTrack');
  expect(effectsContent).toContain('events.nowPlayingChanged.listen');
});

import { commands as aliasCommands } from '@bindings';
import { Effect, Exit } from 'effect';

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

  const exit = await Effect.runPromiseExit(fetchNowPlayingState());
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
