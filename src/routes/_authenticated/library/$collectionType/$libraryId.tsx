import { createFileRoute } from '@tanstack/solid-router';
import {
  LibraryBrowseView,
  libraryKindFromParam,
} from '../../../../features/library/browse';

export const Route = createFileRoute(
  '/_authenticated/library/$collectionType/$libraryId',
)({
  component: LibraryBrowseRoute,
});

function LibraryBrowseRoute() {
  const params = Route.useParams();

  return (
    <LibraryBrowseView
      collectionType={libraryKindFromParam(params().collectionType)}
      libraryId={params().libraryId}
    />
  );
}
