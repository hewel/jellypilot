import { expect, test } from '@rstest/core';
import { RouterContextProvider, createMemoryHistory } from '@tanstack/solid-router';
import { fireEvent, screen } from '@testing-library/dom';
import type { JSX } from 'solid-js';
import { render } from 'solid-js/web';

import { VideoCard } from '../src/components/library/VideoCard';
import { createJellyPilotRouter } from '../src/router';
import { imageSource } from '../src/utils/imageSource';

function renderWithRouter(content: () => JSX.Element) {
  const root = document.createElement('div');
  document.body.append(root);
  const router = createJellyPilotRouter(createMemoryHistory({ initialEntries: ['/library'] }));
  const dispose = render(
    () => <RouterContextProvider router={router}>{content}</RouterContextProvider>,
    root,
  );

  return { dispose, root };
}

test('VideoCard renders image IDs through the JellyPilot image protocol', () => {
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      kind="library"
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

  expect(screen.getByAltText('Protocol Movie artwork')).toHaveAttribute(
    'src',
    imageSource('signed-card-image'),
  );
  expect(screen.getByAltText('Protocol Movie artwork').parentElement).toHaveAttribute(
    'data-aspect',
    'poster',
  );

  dispose();
  root.remove();
});

test('VideoCard falls back when the image protocol load fails', () => {
  const { dispose, root } = renderWithRouter(() => (
    <VideoCard
      kind="library"
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

test('VideoCard overlays copy on poster artwork and keeps copy below video artwork', () => {
  const { dispose, root } = renderWithRouter(() => (
    <>
      <VideoCard
        kind="library"
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
      <VideoCard
        kind="home"
        aspectClass="video"
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
