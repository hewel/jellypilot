import { expect, test } from '@rstest/core';

import { HomeVideoCard as DirectHomeVideoCard } from '../src/components/library/HomeVideoCard';
import { LibraryVideoCard as DirectLibraryVideoCard } from '../src/components/library/LibraryVideoCard';
import { MediaInfoHoverCard as DirectHoverCard } from '../src/components/library/MediaInfoHoverCard';
import * as libraryShared from '../src/components/library/shared';

test('library shared module re-exports HomeVideoCard, LibraryVideoCard, and MediaInfoHoverCard', () => {
  expect(libraryShared.HomeVideoCard).toBe(DirectHomeVideoCard);
  expect(libraryShared.LibraryVideoCard).toBe(DirectLibraryVideoCard);
  expect(libraryShared.MediaInfoHoverCard).toBe(DirectHoverCard);
});
