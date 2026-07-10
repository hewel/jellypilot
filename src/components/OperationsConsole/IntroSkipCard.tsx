import { SegmentedControl } from '@jellypilot/ui';
import { Bot } from 'lucide-solid';
import { Show } from 'solid-js';

import type { IntroSkipperMode } from '../../bindings';
import ConsoleSection from './ConsoleSection';
import { INTRO_SKIPPER_MODES } from './introSkipperModes';
import { useOperationsConsoleStore } from './store';

import * as styles from './shared.css';

interface IntroSkipCardProps {
  currentMode: IntroSkipperMode;
  onModeChange: (mode: IntroSkipperMode) => void;
}

export default function IntroSkipCard(props: IntroSkipCardProps) {
  const [ui] = useOperationsConsoleStore();

  return (
    <ConsoleSection icon={<Bot class={styles.sectionIcon.secondary} />} title="Intro Skip">
      <div class={styles.stack4}>
        <SegmentedControl
          value={props.currentMode}
          aria-label="Intro Skip Mode"
          items={INTRO_SKIPPER_MODES.map((option) => ({
            value: option.mode,
            label: (
              <>
                <span class={styles.choiceTitle}>{option.label}</span>
                <span class={styles.choiceDescription}>{option.description}</span>
              </>
            ),
          }))}
          onValueChange={(value) => {
            const option = INTRO_SKIPPER_MODES.find((candidate) => candidate.mode === value);
            if (option) props.onModeChange(option.mode);
          }}
        />
        <Show when={ui.introSkipperSaving}>
          <p class={styles.saving}>
            <span class={styles.pingDot} />
            Saving preference…
          </p>
        </Show>
        <p class={styles.bodyText}>Changes take effect after restarting MPV.</p>
        <Show when={ui.introSkipperError}>
          {(message) => <p class={styles.errorPanel}>{message()}</p>}
        </Show>
      </div>
    </ConsoleSection>
  );
}
