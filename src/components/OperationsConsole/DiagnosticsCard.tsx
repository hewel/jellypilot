import { ClipboardList } from 'lucide-solid';
import DiagnosticsPanel from '../DiagnosticsPanel';
import { SectionCard } from '../ui';
import { useOperationsConsoleStore } from './store';

export default function DiagnosticsCard() {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <SectionCard
      icon={<ClipboardList class="h-6 w-6" />}
      title="Diagnostics"
      trailing={
        <button
          type="button"
          class="btn-text min-w-0 px-3"
          onClick={actions.toggleDiagnostics}
          aria-expanded={ui.diagnosticsExpanded}
          aria-label="Toggle diagnostics"
        >
          {ui.diagnosticsExpanded ? 'Collapse' : 'Expand'}
        </button>
      }
    >
      <DiagnosticsPanel compact={!ui.diagnosticsExpanded} />
    </SectionCard>
  );
}
