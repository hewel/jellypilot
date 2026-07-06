import { ClipboardList } from 'lucide-solid';

import DiagnosticsPanel from '../DiagnosticsPanel';
import { Button, SectionCard } from '../ui';
import { useOperationsConsoleStore } from './store';

import * as styles from './DiagnosticsCard.css';
import * as shared from './shared.css';

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
          class={styles.toggleButton}
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
