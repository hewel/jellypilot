// @rstest-environment jsdom
import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { RouterContextProvider, createMemoryHistory } from '@tanstack/solid-router';
import { fireEvent, screen } from '@testing-library/dom';
import type { JSX } from 'solid-js';
import { render } from 'solid-js/web';

import { HomeVideoCard } from '../src/components/library/HomeVideoCard';
import { LibraryVideoCard } from '../src/components/library/LibraryVideoCard';
import { PopupRoot } from '../src/components/ui/PopupRoot';
import { createJellyPilotRouter } from '../src/router';
import { resetImageProxyBase, setImageProxyBase } from '../src/utils/imageSource';
import { TestQueryProvider } from './query-client';

beforeEach(() => setImageProxyBase('http://127.0.0.1:43127'));
afterEach(() => {
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
          <RouterContextProvider router={router}>{content}</RouterContextProvider>
        </PopupRoot>
      </TestQueryProvider>
    ),
    root,
  );

  return { dispose, root };
}

test('LibraryVideoCard renders image IDs through the localhost image proxy', () => {
  const { dispose, root } = renderWithRouter(() => (
    <LibraryVideoCard
      collectionType="movies"
      item={{
        artworkImageId: 'signed-card-image',
        episodeNumber: null,
        favorite: false,
        id: 'movie-1',
        itemType: 'Movie',
        name: 'Protocol Movie',
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

test('LibraryVideoCard waits for the image proxy before rendering artwork', () => {
  resetImageProxyBase();
  const { dispose, root } = renderWithRouter(() => (
    <LibraryVideoCard
      collectionType="movies"
      item={{
        artworkImageId: 'delayed-card-image',
        episodeNumber: null,
        favorite: false,
        id: 'movie-delayed',
        itemType: 'Movie',
        name: 'Delayed Movie',
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

test('LibraryVideoCard falls back when the proxy image load fails', () => {
  const { dispose, root } = renderWithRouter(() => (
    <LibraryVideoCard
      collectionType="movies"
      item={{
        artworkImageId: 'broken-card-image',
        episodeNumber: null,
        favorite: false,
        id: 'movie-1',
        itemType: 'Movie',
        name: 'Broken Movie',
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

test('LibraryVideoCard overlays copy on poster artwork and HomeVideoCard keeps copy below video artwork', () => {
  const { dispose, root } = renderWithRouter(() => (
    <>
      <LibraryVideoCard
        collectionType="movies"
        item={{
          artworkImageId: null,
          episodeNumber: null,
          favorite: false,
          id: 'movie-1',
          itemType: 'Movie',
          name: 'Overlay Movie',
          played: true,
          playedPercentage: null,
          productionYear: 2024,
          resumePositionSeconds: null,
          runtimeSeconds: 7200,
          seasonNumber: null,
          seriesId: null,
          seriesName: null,
        }}
      />
      <HomeVideoCard
        rowKind="latestEpisodes"
        item={{
          artworkImageId: null,
          episodeNumber: 2,
          favorite: false,
          id: 'episode-1',
          itemType: 'Episode',
          name: 'Overlay Episode',
          played: false,
          playedPercentage: null,
          productionYear: 2024,
          resumePositionSeconds: null,
          runtimeSeconds: 2700,
          seasonNumber: 1,
          seriesId: 'series-1',
          seriesName: 'Overlay Series',
        }}
      />
    </>
  ));

  const posterTitle = screen.getByText('Overlay Movie');
  expect(posterTitle).toBeVisible();
  expect(posterTitle.closest('[data-aspect="poster"]')).not.toBeNull();
  expect(screen.getByText('2024').closest('[data-aspect="poster"]')).not.toBeNull();
  expect(screen.getByText('Overlay Episode').closest('[data-aspect]')).toBeNull();
  expect(screen.getByRole('img', { name: 'Played' })).toBeVisible();

  dispose();
  root.remove();
});

test('HomeVideoCard renders reference-first Continue Watching metadata and direct resume', () => {
  const onResume = rstest.fn();
  const { dispose, root } = renderWithRouter(() => (
    <HomeVideoCard
      rowKind="continueWatching"
      onResume={onResume}
      item={{
        artworkImageId: null,
        episodeNumber: 4,
        favorite: true,
        id: 'episode-4',
        itemType: 'Episode',
        name: 'A Quiet Return',
        played: true,
        playedPercentage: 25,
        productionYear: 2024,
        resumePositionSeconds: 120,
        runtimeSeconds: 3600,
        seasonNumber: 1,
        seriesId: 'series-1',
        seriesName: 'Silent Echoes',
      }}
    />
  ));

  const title = screen.getByText('Silent Echoes • S1 E4');
  const resumeButton = screen.getByRole('button', { name: 'Resume A Quiet Return' });
  expect(title).toBeVisible();
  expect(screen.getByText('58 mins remaining')).toBeVisible();
  expect(screen.queryByRole('img', { name: 'Played' })).toBeNull();
  expect(
    screen.getByRole('progressbar', { name: 'A Quiet Return watch progress' }),
  ).toHaveAttribute('aria-valuenow', '25');
  expect(root.querySelector('[data-play-badge]')).not.toBeNull();
  expect(screen.queryByText('No artwork')).toBeNull();
  expect(resumeButton.contains(title)).toBe(false);

  fireEvent.click(title);
  expect(onResume).not.toHaveBeenCalled();
  fireEvent.click(resumeButton);
  expect(onResume).toHaveBeenCalledTimes(1);

  dispose();
  root.remove();
});

test('HomeVideoCard title links episodes to series detail and movies to movie detail', () => {
  const { dispose, root } = renderWithRouter(() => (
    <>
      <HomeVideoCard
        rowKind="continueWatching"
        onResume={() => undefined}
        item={{
          artworkImageId: null,
          episodeNumber: 4,
          favorite: false,
          id: 'episode-4',
          itemType: 'Episode',
          name: 'A Quiet Return',
          played: false,
          playedPercentage: null,
          productionYear: 2024,
          resumePositionSeconds: 120,
          runtimeSeconds: 3600,
          seasonNumber: 1,
          seriesId: 'series-1',
          seriesName: 'Silent Echoes',
        }}
      />
      <HomeVideoCard
        rowKind="latestMovies"
        item={{
          artworkImageId: null,
          episodeNumber: null,
          favorite: false,
          id: 'movie-1',
          itemType: 'Movie',
          name: 'Linked Movie',
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
    </>
  ));

  const resumeTitleLink = screen.getByRole('link', { name: 'Open details for A Quiet Return' });
  expect(resumeTitleLink).toHaveAttribute('href', '/library/shows/series-1');
  expect(resumeTitleLink.contains(screen.getByText('Silent Echoes • S1 E4'))).toBe(true);

  const movieTitleLink = screen.getByRole('link', { name: 'Open details for Linked Movie' });
  expect(movieTitleLink).toHaveAttribute('href', '/library/items/movie-1');
  expect(movieTitleLink.contains(screen.getByText('Linked Movie • Movie'))).toBe(true);

  dispose();
  root.remove();
});

test('HomeVideoCard derives progress and falls back to detail without a saved resume position', () => {
  const { dispose, root } = renderWithRouter(() => (
    <>
      <HomeVideoCard
        rowKind="continueWatching"
        onResume={() => undefined}
        item={{
          artworkImageId: null,
          episodeNumber: null,
          favorite: false,
          id: 'movie-derived',
          itemType: 'Movie',
          name: 'Derived Progress',
          played: false,
          playedPercentage: null,
          productionYear: 2024,
          resumePositionSeconds: 900,
          runtimeSeconds: 3600,
          seasonNumber: null,
          seriesId: null,
          seriesName: null,
        }}
      />
      <HomeVideoCard
        rowKind="continueWatching"
        onResume={() => undefined}
        item={{
          artworkImageId: null,
          episodeNumber: null,
          favorite: false,
          id: 'movie-no-resume',
          itemType: 'Movie',
          name: 'No Resume',
          played: false,
          playedPercentage: null,
          productionYear: 2024,
          resumePositionSeconds: null,
          runtimeSeconds: 3600,
          seasonNumber: null,
          seriesId: null,
          seriesName: null,
        }}
      />
    </>
  ));

  expect(
    screen.getByRole('progressbar', { name: 'Derived Progress watch progress' }),
  ).toHaveAttribute('aria-valuenow', '25');
  expect(screen.getByRole('link', { name: 'Open No Resume' })).toHaveAttribute(
    'href',
    '/library/items/movie-no-resume',
  );
  expect(screen.queryByRole('button', { name: 'Resume No Resume' })).toBeNull();
  expect(
    screen.getByRole('link', { name: 'Open No Resume' }).querySelector('[data-play-badge]'),
  ).toBeNull();
  expect(screen.getByText('No artwork')).toBeVisible();

  dispose();
  root.remove();
});

test('HomeVideoCard exposes a disabled busy state while resume starts', () => {
  const { dispose, root } = renderWithRouter(() => (
    <HomeVideoCard
      rowKind="continueWatching"
      busy
      resumeDisabled
      onResume={() => undefined}
      item={{
        artworkImageId: null,
        episodeNumber: null,
        favorite: false,
        id: 'movie-busy',
        itemType: 'Movie',
        name: 'Busy Movie',
        played: false,
        playedPercentage: 40,
        productionYear: 2024,
        resumePositionSeconds: 120,
        runtimeSeconds: 3600,
        seasonNumber: null,
        seriesId: null,
        seriesName: null,
      }}
    />
  ));

  const button = screen.getByRole('button', { name: 'Starting Busy Movie' });
  expect(button).toBeDisabled();
  expect(button).toHaveAttribute('aria-busy', 'true');
  expect(screen.getByText('Starting…')).toBeVisible();
  expect(root.querySelector('[data-play-badge]')).toBeNull();

  dispose();
  root.remove();
});
