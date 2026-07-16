import { ClipboardList } from 'lucide-solid';

import DiagnosticsPanel from '../DiagnosticsPanel';
import { Button, SectionCard } from '../ui';
import * as rootStyles from './DiagnosticsCard.styles';
import * as shared from './shared.styles';
import { useOperationsConsoleStore } from './store';

export default function DiagnosticsCard() {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <SectionCard
      icon={<ClipboardList class={shared.sectionIcon.plain} />}
      title="Diagnostics"
      trailing={
        <Button
          type="button"
          variant="text"
          class={rootStyles.toggleButton}
          onClick={actions.toggleDiagnostics}
          aria-expanded={ui.diagnosticsExpanded}
          aria-label="Toggle diagnostics"
        >
          {ui.diagnosticsExpanded ? 'Collapse' : 'Expand'}
        </Button>
      }
    >
      <DiagnosticsPanel compact={!ui.diagnosticsExpanded} />
    </SectionCard>
  );
}
