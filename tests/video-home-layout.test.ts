import { expect, test } from '@rstest/core';

import {
  isValidVideoHomeResumePosition,
  videoHomeAspect,
  videoHomeColumnCount,
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
  expect(videoHomeColumnCount('video', 899)).toBe(2);
  expect(videoHomeColumnCount('video', 900)).toBe(3);
  expect(videoHomeColumnCount('video', 1399)).toBe(3);
  expect(videoHomeColumnCount('video', 1400)).toBe(4);
  expect(videoHomeColumnCount('video', 2199)).toBe(4);
  expect(videoHomeColumnCount('video', 2200)).toBe(5);
});

test('Video Home poster capacity follows measured row width', () => {
  expect(videoHomeColumnCount('poster', 559)).toBe(2);
  expect(videoHomeColumnCount('poster', 560)).toBe(3);
  expect(videoHomeColumnCount('poster', 839)).toBe(3);
  expect(videoHomeColumnCount('poster', 840)).toBe(4);
  expect(videoHomeColumnCount('poster', 1099)).toBe(4);
  expect(videoHomeColumnCount('poster', 1100)).toBe(5);
  expect(videoHomeColumnCount('poster', 1399)).toBe(5);
  expect(videoHomeColumnCount('poster', 1400)).toBe(6);
  expect(videoHomeColumnCount('poster', 1699)).toBe(6);
  expect(videoHomeColumnCount('poster', 1700)).toBe(7);
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
