import { createFileRoute } from '@tanstack/solid-router';
import { LibraryLanding } from '../../../features/library/home';

export const Route = createFileRoute('/_authenticated/library/')({
  component: LibraryLanding,
});
