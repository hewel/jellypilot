import { createFileRoute } from '@tanstack/solid-router';
import { DiagnosticsRoute } from '../../features/routes/diagnostics';

export const Route = createFileRoute('/_authenticated/diagnostics')({
  component: DiagnosticsRoute,
});
