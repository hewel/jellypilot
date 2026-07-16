import { expect, test } from '@rstest/core';

import { MediaInfoHoverCard as DirectHoverCard } from '../src/components/library/MediaInfoHoverCard';
import * as libraryShared from '../src/components/library/shared';
import { VideoCard as DirectVideoCard } from '../src/components/library/VideoCard';

test('library shared module re-exports VideoCard and MediaInfoHoverCard', () => {
  // Pre-migration public barrel surface — preserve for existing import paths.
  expect(libraryShared.VideoCard).toBe(DirectVideoCard);
  expect(libraryShared.MediaInfoHoverCard).toBe(DirectHoverCard);
});
