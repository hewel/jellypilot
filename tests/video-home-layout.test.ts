import { expect, test } from '@rstest/core';

import {
  isValidVideoHomeResumePosition,
  videoHomeAspect,
  videoHomeColumnCount,
  videoHomePlaybackDecision,
} from '../src/utils/videoHomeLayout';

test('Video Home maps row kinds to media-appropriate artwork', () => {
  expect(videoHomeAspect('continueWatching')).toBe('video');
  expect(videoHomeAspect('nextUp')).toBe('video');
  expect(videoHomeAspect('latestEpisodes')).toBe('video');
  expect(videoHomeAspect('latestMovies')).toBe('poster');
});

test('Video Home landscape capacity follows measured row width', () => {
  expect(videoHomeColumnCount('video', 559)).toBe(1);
  expect(videoHomeColumnCount('video', 560)).toBe(2);
  expect(videoHomeColumnCount('video', 819)).toBe(2);
  expect(videoHomeColumnCount('video', 820)).toBe(3);
  expect(videoHomeColumnCount('video', 1119)).toBe(3);
  expect(videoHomeColumnCount('video', 1120)).toBe(4);
  expect(videoHomeColumnCount('video', 1379)).toBe(4);
  expect(videoHomeColumnCount('video', 1380)).toBe(5);
});

test('Video Home poster capacity follows measured row width', () => {
  expect(videoHomeColumnCount('poster', 559)).toBe(2);
  expect(videoHomeColumnCount('poster', 560)).toBe(3);
  expect(videoHomeColumnCount('poster', 699)).toBe(3);
  expect(videoHomeColumnCount('poster', 700)).toBe(4);
  expect(videoHomeColumnCount('poster', 949)).toBe(4);
  expect(videoHomeColumnCount('poster', 950)).toBe(5);
  expect(videoHomeColumnCount('poster', 1159)).toBe(5);
  expect(videoHomeColumnCount('poster', 1160)).toBe(6);
  expect(videoHomeColumnCount('poster', 1389)).toBe(6);
  expect(videoHomeColumnCount('poster', 1390)).toBe(7);
});

test('Video Home capacity uses conservative fallbacks for unknown widths', () => {
  expect(videoHomeColumnCount('video', 0)).toBe(1);
  expect(videoHomeColumnCount('video', Number.NaN)).toBe(1);
  expect(videoHomeColumnCount('poster', -1)).toBe(2);
  expect(videoHomeColumnCount('poster', Number.POSITIVE_INFINITY)).toBe(2);
});

test('Video Home resumes only from finite positions inside a known runtime', () => {
  expect(isValidVideoHomeResumePosition(120, 3600)).toBe(true);
  expect(isValidVideoHomeResumePosition(120, null)).toBe(true);
  expect(isValidVideoHomeResumePosition(0, 3600)).toBe(false);
  expect(isValidVideoHomeResumePosition(Number.NaN, 3600)).toBe(false);
  expect(isValidVideoHomeResumePosition(3600, 3600)).toBe(false);
  expect(isValidVideoHomeResumePosition(3601, 3600)).toBe(false);
});

test('Video Home playback decision resumes positive in-range offsets', () => {
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 120, runtimeSeconds: 3600 })).toEqual({
    mode: 'resume',
    startPositionSeconds: 120,
  });
});

test('Video Home playback decision resumes positive offsets with unknown runtime', () => {
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 120, runtimeSeconds: null })).toEqual({
    mode: 'resume',
    startPositionSeconds: 120,
  });
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 120, runtimeSeconds: 0 })).toEqual({
    mode: 'resume',
    startPositionSeconds: 120,
  });
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 120, runtimeSeconds: -5 })).toEqual({
    mode: 'resume',
    startPositionSeconds: 120,
  });
});

test('Video Home playback decision starts null, zero, negative, and non-finite offsets', () => {
  expect(videoHomePlaybackDecision({ resumePositionSeconds: null, runtimeSeconds: 3600 })).toEqual({
    mode: 'start',
    startPositionSeconds: null,
  });
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 0, runtimeSeconds: 3600 })).toEqual({
    mode: 'start',
    startPositionSeconds: null,
  });
  expect(videoHomePlaybackDecision({ resumePositionSeconds: -30, runtimeSeconds: 3600 })).toEqual({
    mode: 'start',
    startPositionSeconds: null,
  });
  expect(
    videoHomePlaybackDecision({ resumePositionSeconds: Number.NaN, runtimeSeconds: 3600 }),
  ).toEqual({ mode: 'start', startPositionSeconds: null });
  expect(
    videoHomePlaybackDecision({
      resumePositionSeconds: Number.POSITIVE_INFINITY,
      runtimeSeconds: 3600,
    }),
  ).toEqual({ mode: 'start', startPositionSeconds: null });
});

test('Video Home playback decision starts offsets at or past the runtime', () => {
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 3600, runtimeSeconds: 3600 })).toEqual({
    mode: 'start',
    startPositionSeconds: null,
  });
  expect(videoHomePlaybackDecision({ resumePositionSeconds: 3601, runtimeSeconds: 3600 })).toEqual({
    mode: 'start',
    startPositionSeconds: null,
  });
});
