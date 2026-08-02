// @rstest-environment jsdom
import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { RouterContextProvider, createMemoryHistory } from '@tanstack/solid-router';
import { fireEvent, screen } from '@testing-library/dom';
import type { JSX } from 'solid-js';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import { AuthenticatedBootstrapProvider } from '../src/components/AuthenticatedBootstrap';
import { VideoCard } from '../src/components/library/VideoCard';
import { videoCardProgress, videoCardSubtitle } from '../src/components/library/videoCardModel';
import { PopupRoot } from '../src/components/ui/PopupRoot';
import { createJellyPilotRouter } from '../src/router';
import { resetImageProxyBase, setImageProxyBase } from '../src/utils/imageSource';
import { TestQueryProvider } from './query-client';

beforeEach(() => {
  setImageProxyBase('http://127.0.0.1:43127');
  rstest.spyOn(commands, 'serverGetState').mockResolvedValue({
    capabilities: {
      introSkipper: true,
      quickConnect: true,
      remoteControl: true,
      remoteControlAvailable: true,
      remoteControlWarning: null,
    },
    connected: true,
    provider: 'jellyfin',
    serverName: 'Jellyfin Home',
    serverUrl: 'https://jellyfin.example.com',
    userId: 'user-1',
    userName: 'Ada',
  });
  rstest.spyOn(commands, 'appLocalServices').mockResolvedValue({
    imageProxyBase: 'http://127.0.0.1:43127',
  });
});
afterEach(() => {
  rstest.restoreAllMocks();
  resetImageProxyBase();
  document.body.innerHTML = '';
});

function renderWithRouter(content: () => JSX.Element) {
  const root = document.createElement('div');
  document.body.append(root);
  const router = createJellyPilotRouter(createMemoryHistory({ initialEntries: ['/library'] }));
  const dispose = render(
    () => (
      <TestQueryProvider>
        <PopupRoot>
          <RouterContextProvider router={router}>
            {() => <AuthenticatedBootstrapProvider>{content()}</AuthenticatedBootstrapProvider>}
          </RouterContextProvider>
        </PopupRoot>
      </TestQueryProvider>
    ),
    root,
  );

  return { dispose, root };
}

test('VideoCard renders image IDs through the localhost image proxy', () => {
  const item = {
    artworkImageId: 'signed-card-image',
    episodeNumber: null,
    favorite: false,
    id: 'movie-1',
    itemType: 'Movie',
    name: 'Protocol Movie',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: null,
    runtimeSeconds: 7200,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      item={item}
      aspect="poster"
      action={{ kind: 'open' }}
      subtitle={videoCardSubtitle(item, { kind: 'browse' })}
      badges={{ favorite: true, played: true }}
    />
  ));

  expect(screen.getByAltText('Protocol Movie artwork').getAttribute('src')).toContain(
    'signed-card-image',
  );
  expect(screen.getByAltText('Protocol Movie artwork').parentElement).toHaveAttribute(
    'data-aspect',
    'poster',
  );

  dispose();
  root.remove();
});

test('VideoCard waits for the image proxy before rendering artwork', () => {
  resetImageProxyBase();
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      aspect="poster"
      action={{ kind: 'open' }}
      subtitle="2024"
      badges={{ favorite: true, played: true }}
      item={{
        artworkImageId: 'delayed-card-image',
        episodeNumber: null,
        favorite: false,
        id: 'movie-delayed',
        itemType: 'Movie',
        name: 'Delayed Movie',
        overview: null,
        played: false,
        playedPercentage: null,
        productionYear: 2024,
        resumePositionSeconds: null,
        runtimeSeconds: 7200,
        seasonNumber: null,
        seriesId: null,
        seriesName: null,
      }}
    />
  ));

  expect(screen.queryByAltText('Delayed Movie artwork')).toBeNull();
  expect(screen.getByText('No artwork')).toBeVisible();

  setImageProxyBase('http://127.0.0.1:43127');

  expect(screen.getByAltText('Delayed Movie artwork').getAttribute('src')).toContain(
    'delayed-card-image',
  );

  dispose();
  root.remove();
});

test('VideoCard falls back when the proxy image load fails', () => {
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      aspect="poster"
      action={{ kind: 'open' }}
      subtitle="2024"
      badges={{ favorite: true, played: true }}
      item={{
        artworkImageId: 'broken-card-image',
        episodeNumber: null,
        favorite: false,
        id: 'movie-1',
        itemType: 'Movie',
        name: 'Broken Movie',
        overview: null,
        played: false,
        playedPercentage: null,
        productionYear: 2024,
        resumePositionSeconds: null,
        runtimeSeconds: 7200,
        seasonNumber: null,
        seriesId: null,
        seriesName: null,
      }}
    />
  ));

  fireEvent.error(screen.getByAltText('Broken Movie artwork'));
  expect(screen.getByText('No artwork')).toBeVisible();

  dispose();
  root.remove();
});

test('VideoCard keeps poster and video metadata below framed artwork', () => {
  const posterItem = {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'movie-1',
    itemType: 'Movie',
    name: 'Poster Movie',
    overview: null,
    played: true,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: null,
    runtimeSeconds: 7200,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const videoItem = {
    artworkImageId: null,
    episodeNumber: 2,
    favorite: false,
    id: 'episode-1',
    itemType: 'Episode',
    name: 'Video Episode',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: null,
    runtimeSeconds: 2700,
    seasonNumber: 1,
    seriesId: 'series-1',
    seriesName: 'Video Series',
  };
  const { dispose, root } = renderWithRouter(() => (
    <>
      <VideoCard
        item={posterItem}
        aspect="poster"
        action={{ kind: 'open' }}
        subtitle={videoCardSubtitle(posterItem, { kind: 'browse' })}
        badges={{ favorite: true, played: true }}
      />
      <VideoCard
        item={videoItem}
        aspect="video"
        action={{ kind: 'open' }}
        subtitle={videoCardSubtitle(videoItem, { kind: 'homeRow', rowKind: 'latestEpisodes' })}
      />
    </>
  ));

  const posterTitle = screen.getByText('Poster Movie');
  expect(posterTitle).toBeVisible();
  expect(posterTitle.closest('[data-aspect]')).toBeNull();
  expect(screen.getByText('2024').closest('[data-aspect]')).toBeNull();
  expect(screen.getByText('Video Series • Video Episode').closest('[data-aspect]')).toBeNull();
  expect(screen.getByRole('img', { name: 'Played' })).toBeVisible();

  dispose();
  root.remove();
});

test('VideoCard renders reference-first Continue Watching metadata and direct resume', () => {
  const onPlay = rstest.fn();
  const item = {
    artworkImageId: null,
    episodeNumber: 4,
    favorite: true,
    id: 'episode-4',
    itemType: 'Episode',
    name: 'A Quiet Return',
    overview: null,
    played: true,
    playedPercentage: 25,
    productionYear: 2024,
    resumePositionSeconds: 120,
    runtimeSeconds: 3600,
    seasonNumber: 1,
    seriesId: 'series-1',
    seriesName: 'Silent Echoes',
  };
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      item={item}
      aspect="video"
      action={{ kind: 'play', onPlay }}
      subtitle={videoCardSubtitle(item, { kind: 'homeRow', rowKind: 'continueWatching' })}
      progress={videoCardProgress(item)}
    />
  ));

  const title = screen.getByText('Silent Echoes • A Quiet Return');
  const resumeButton = screen.getByRole('button', { name: 'Resume A Quiet Return' });
  expect(title).toBeVisible();
  expect(screen.getByText('58 mins remaining · S1 E4')).toBeVisible();
  expect(screen.queryByRole('img', { name: 'Played' })).toBeNull();
  expect(
    screen.getByRole('progressbar', { name: 'A Quiet Return watch progress' }),
  ).toHaveAttribute('aria-valuenow', '25');
  expect(root.querySelector('[data-play-badge]')).not.toBeNull();
  expect(screen.queryByText('No artwork')).toBeNull();
  expect(resumeButton.contains(title)).toBe(false);

  fireEvent.click(title);
  expect(onPlay).not.toHaveBeenCalled();
  fireEvent.click(resumeButton);
  expect(onPlay).toHaveBeenCalledTimes(1);

  dispose();
  root.remove();
});

test('VideoCard title links episodes to series detail and movies to movie detail', () => {
  const episodeItem = {
    artworkImageId: null,
    episodeNumber: 4,
    favorite: false,
    id: 'episode-4',
    itemType: 'Episode',
    name: 'A Quiet Return',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: 120,
    runtimeSeconds: 3600,
    seasonNumber: 1,
    seriesId: 'series-1',
    seriesName: 'Silent Echoes',
  };
  const movieItem = {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'movie-1',
    itemType: 'Movie',
    name: 'Linked Movie',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: null,
    runtimeSeconds: 7200,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const { dispose, root } = renderWithRouter(() => (
    <>
      <VideoCard
        item={episodeItem}
        aspect="video"
        action={{ kind: 'play', onPlay: () => undefined }}
        subtitle={videoCardSubtitle(episodeItem, { kind: 'homeRow', rowKind: 'continueWatching' })}
        progress={videoCardProgress(episodeItem)}
      />
      <VideoCard
        item={movieItem}
        aspect="poster"
        action={{ kind: 'open' }}
        subtitle={videoCardSubtitle(movieItem, { kind: 'homeRow', rowKind: 'latestMovies' })}
      />
    </>
  ));

  const resumeTitleLink = screen.getByRole('link', { name: 'Open details for A Quiet Return' });
  expect(resumeTitleLink).toHaveAttribute('href', '/library/shows/series-1');
  expect(resumeTitleLink.contains(screen.getByText('Silent Echoes • A Quiet Return'))).toBe(true);

  const movieTitleLink = screen.getByRole('link', { name: 'Open details for Linked Movie' });
  expect(movieTitleLink).toHaveAttribute('href', '/library/items/movie-1');
  expect(movieTitleLink.contains(screen.getByText('Linked Movie'))).toBe(true);

  dispose();
  root.remove();
});

test('VideoCard derives progress and plays from zero without a saved resume position', () => {
  const derivedItem = {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'movie-derived',
    itemType: 'Movie',
    name: 'Derived Progress',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: 900,
    runtimeSeconds: 3600,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const noResumeItem = {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'movie-no-resume',
    itemType: 'Movie',
    name: 'No Resume',
    overview: null,
    played: false,
    playedPercentage: 25,
    productionYear: 2024,
    resumePositionSeconds: null,
    runtimeSeconds: 3600,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const zeroResumeItem = {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'movie-zero-resume',
    itemType: 'Movie',
    name: 'Zero Resume',
    overview: null,
    played: false,
    playedPercentage: 40,
    productionYear: 2024,
    resumePositionSeconds: 0,
    runtimeSeconds: 3600,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const { dispose, root } = renderWithRouter(() => (
    <>
      <VideoCard
        item={derivedItem}
        aspect="video"
        action={{ kind: 'play', onPlay: () => undefined }}
        subtitle={videoCardSubtitle(derivedItem, { kind: 'homeRow', rowKind: 'continueWatching' })}
        progress={videoCardProgress(derivedItem)}
      />
      <VideoCard
        item={noResumeItem}
        aspect="video"
        action={{ kind: 'play', onPlay: () => undefined }}
        subtitle={videoCardSubtitle(noResumeItem, { kind: 'homeRow', rowKind: 'continueWatching' })}
        progress={videoCardProgress(noResumeItem)}
      />
      <VideoCard
        item={zeroResumeItem}
        aspect="video"
        action={{ kind: 'play', onPlay: () => undefined }}
        subtitle={videoCardSubtitle(zeroResumeItem, {
          kind: 'homeRow',
          rowKind: 'continueWatching',
        })}
        progress={videoCardProgress(zeroResumeItem)}
      />
    </>
  ));

  expect(
    screen.getByRole('progressbar', { name: 'Derived Progress watch progress' }),
  ).toHaveAttribute('aria-valuenow', '25');

  // No saved offset: the card stays directly actionable as Play, keeps the
  // server-supplied percentage text/progress, and is not a Details-only link.
  const playButton = screen.getByRole('button', { name: 'Play No Resume' });
  expect(playButton).toBeVisible();
  expect(screen.queryByRole('link', { name: 'Open No Resume' })).toBeNull();
  expect(screen.getByText('25% watched')).toBeVisible();
  expect(screen.getByRole('progressbar', { name: 'No Resume watch progress' })).toHaveAttribute(
    'aria-valuenow',
    '25',
  );
  expect(playButton.querySelector('[data-play-badge]')).not.toBeNull();

  // Zero offset behaves like a missing offset: Play with preserved progress.
  const zeroPlayButton = screen.getByRole('button', { name: 'Play Zero Resume' });
  expect(zeroPlayButton).toBeVisible();
  expect(screen.getByText('40% watched')).toBeVisible();
  expect(screen.getByRole('progressbar', { name: 'Zero Resume watch progress' })).toHaveAttribute(
    'aria-valuenow',
    '40',
  );
  expect(screen.queryAllByText('No artwork')).toHaveLength(0);

  dispose();
  root.remove();
});

test('VideoCard resumes Next Up episodes while keeping the episode subtitle', () => {
  const onPlay = rstest.fn();
  const item = {
    artworkImageId: null,
    episodeNumber: 5,
    favorite: false,
    id: 'episode-5',
    itemType: 'Episode',
    name: 'The Crossing',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: 900,
    runtimeSeconds: 3600,
    seasonNumber: 2,
    seriesId: 'series-1',
    seriesName: 'Harbor Line',
  };
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      item={item}
      aspect="video"
      action={{ kind: 'play', onPlay }}
      subtitle={videoCardSubtitle(item, { kind: 'homeRow', rowKind: 'nextUp' })}
      progress={videoCardProgress(item)}
    />
  ));

  const resumeButton = screen.getByRole('button', { name: 'Resume The Crossing' });
  expect(resumeButton).toBeVisible();
  // Next Up titles carry series plus episode name; the episode code moves to
  // the subtitle and remaining time never appears here.
  expect(screen.getByText('Harbor Line • The Crossing')).toBeVisible();
  expect(screen.getByText('S2 E5')).toBeVisible();
  expect(screen.queryByText(/remaining/)).toBeNull();
  expect(screen.getByRole('progressbar', { name: 'The Crossing watch progress' })).toHaveAttribute(
    'aria-valuenow',
    '25',
  );
  expect(root.querySelector('[data-play-badge]')).not.toBeNull();

  fireEvent.click(resumeButton);
  expect(onPlay).toHaveBeenCalledTimes(1);

  dispose();
  root.remove();
});

test('VideoCard plays Next Up episodes from zero without a resume offset', () => {
  const item = {
    artworkImageId: null,
    episodeNumber: 5,
    favorite: false,
    id: 'episode-5',
    itemType: 'Episode',
    name: 'The Crossing',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: null,
    runtimeSeconds: 3600,
    seasonNumber: 2,
    seriesId: 'series-1',
    seriesName: 'Harbor Line',
  };
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      item={item}
      aspect="video"
      action={{ kind: 'play', onPlay: () => undefined }}
      subtitle={videoCardSubtitle(item, { kind: 'homeRow', rowKind: 'nextUp' })}
      progress={null}
    />
  ));

  expect(screen.getByRole('button', { name: 'Play The Crossing' })).toBeVisible();
  expect(screen.getByText('Harbor Line • The Crossing')).toBeVisible();
  expect(screen.getByText('S2 E5')).toBeVisible();
  // Start-mode Next Up cards do not invent a progress bar.
  expect(screen.queryByRole('progressbar')).toBeNull();
  expect(root.querySelector('[data-play-badge]')).not.toBeNull();

  dispose();
  root.remove();
});

test('VideoCard exposes a disabled busy state while playback starts', () => {
  const busyItem = {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'movie-busy',
    itemType: 'Movie',
    name: 'Busy Movie',
    overview: null,
    played: false,
    playedPercentage: 40,
    productionYear: 2024,
    resumePositionSeconds: 120,
    runtimeSeconds: 3600,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
  };
  const waitingItem = {
    artworkImageId: null,
    episodeNumber: 2,
    favorite: false,
    id: 'episode-waiting',
    itemType: 'Episode',
    name: 'Waiting Episode',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: 2024,
    resumePositionSeconds: 60,
    runtimeSeconds: 1800,
    seasonNumber: 1,
    seriesId: 'series-1',
    seriesName: 'Waiting Show',
  };
  const { dispose, root } = renderWithRouter(() => (
    <>
      <VideoCard
        item={busyItem}
        aspect="video"
        action={{ kind: 'play', busy: true, disabled: true, onPlay: () => undefined }}
        subtitle={videoCardSubtitle(busyItem, { kind: 'homeRow', rowKind: 'continueWatching' })}
        progress={videoCardProgress(busyItem)}
      />
      <VideoCard
        item={waitingItem}
        aspect="video"
        action={{ kind: 'play', disabled: true, onPlay: () => undefined }}
        subtitle={videoCardSubtitle(waitingItem, { kind: 'homeRow', rowKind: 'nextUp' })}
        progress={videoCardProgress(waitingItem)}
      />
    </>
  ));

  const button = screen.getByRole('button', { name: 'Starting Busy Movie' });
  expect(button).toBeDisabled();
  expect(button).toHaveAttribute('aria-busy', 'true');
  expect(screen.getByText('Starting…')).toBeVisible();
  expect(button.querySelector('[data-play-badge]')).toBeNull();

  // Another direct card disabled by the same launch keeps its resting badge.
  const waiting = screen.getByRole('button', { name: 'Resume Waiting Episode' });
  expect(waiting).toBeDisabled();
  expect(waiting.querySelector('[data-play-badge]')).not.toBeNull();

  dispose();
  root.remove();
});
