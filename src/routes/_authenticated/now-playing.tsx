import { createFileRoute } from '@tanstack/solid-router';
import { NowPlayingRoute } from '../../features/routes/now-playing';

export const Route = createFileRoute('/_authenticated/now-playing')({
  component: NowPlayingRoute,
});
