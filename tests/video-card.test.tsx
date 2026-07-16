import { expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { VideoCard } from '../src/components/library/VideoCard';
import { imageSource } from '../src/utils/imageSource';

test('VideoCard renders image IDs through the JellyPilot image protocol', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
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
    ),
    root,
  );

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
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
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
    ),
    root,
  );

  fireEvent.error(screen.getByAltText('Broken Movie artwork'));

  expect(screen.getByText('No artwork')).toBeVisible();

  dispose();
  root.remove();
});
