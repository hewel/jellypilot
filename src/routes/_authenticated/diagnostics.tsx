import { createFileRoute } from '@tanstack/solid-router';

import DiagnosticsPanel from '../../components/DiagnosticsPanel';
import { Card } from '../../components/ui';

function DiagnosticsRoute() {
  return (
    <Card as="section" variant="elevated" class="space-y-5" aria-labelledby="diagnostics-title">
      <div>
        <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
          Runtime
        </p>
        <h1
          id="diagnostics-title"
          class="font-display text-[32px] leading-[40px] font-bold tracking-tight"
        >
          Diagnostics
        </h1>
      </div>
      <DiagnosticsPanel />
    </Card>
  );
}

export const Route = createFileRoute('/_authenticated/diagnostics')({
  component: DiagnosticsRoute,
});
