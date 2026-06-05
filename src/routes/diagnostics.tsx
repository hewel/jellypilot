import DiagnosticsPanel from '../components/DiagnosticsPanel';

export function DiagnosticsRoute() {
  return (
    <section
      class="card-elevated space-y-5"
      aria-labelledby="diagnostics-title"
    >
      <div>
        <p class="text-label-small text-secondary">Runtime</p>
        <h1 id="diagnostics-title" class="text-headline-large">
          Diagnostics
        </h1>
      </div>
      <DiagnosticsPanel />
    </section>
  );
}
