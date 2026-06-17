import { createFileRoute } from '@tanstack/solid-router';
import { LibraryItemDetailView } from '../../../../features/library/item-detail';

export const Route = createFileRoute('/_authenticated/library/items/$itemId')({
  component: LibraryItemDetailRoute,
});

function LibraryItemDetailRoute() {
  const params = Route.useParams();

  return <LibraryItemDetailView itemId={params().itemId} />;
}
