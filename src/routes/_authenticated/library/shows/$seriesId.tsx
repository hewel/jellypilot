import { createFileRoute } from '@tanstack/solid-router';
import { LibraryShowDetailView } from '../../../../features/library/show-detail';

export const Route = createFileRoute('/_authenticated/library/shows/$seriesId')(
  {
    component: LibraryShowDetailRoute,
  },
);

function LibraryShowDetailRoute() {
  const params = Route.useParams();

  return <LibraryShowDetailView seriesId={params().seriesId} />;
}
