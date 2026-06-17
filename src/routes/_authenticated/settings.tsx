import { createFileRoute } from '@tanstack/solid-router';
import { SettingsRoute } from '../../features/routes/settings';

export const Route = createFileRoute('/_authenticated/settings')({
  component: SettingsRoute,
});
