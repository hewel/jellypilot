import { createFileRoute } from '@tanstack/solid-router';
import LogPanel from '../../components/LogPanel';

export const Route = createFileRoute('/settings/logs')({
  component: LogPanel,
});
