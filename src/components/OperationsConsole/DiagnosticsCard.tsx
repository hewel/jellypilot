import { Button } from '@jellypilot/ui';
import { ClipboardList } from 'lucide-solid';

import DiagnosticsPanel from '../DiagnosticsPanel';
import ConsoleSection from './ConsoleSection';
import { useOperationsConsoleStore } from './store';

import * as styles from './DiagnosticsCard.css';
import * as shared from './shared.css';

export default function DiagnosticsCard() {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <ConsoleSection
      icon={<ClipboardList class={shared.sectionIcon.plain} />}
      title="Diagnostics"
      trailing={
        <Button
          type="button"
          variant="ghost"
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
    </ConsoleSection>
  );
}
