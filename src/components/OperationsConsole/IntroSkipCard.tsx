import { Bot } from 'lucide-solid';
import { For, Show } from 'solid-js';

import type { IntroSkipperMode } from '../../bindings';
import { SectionCard } from '../ui';
import { INTRO_SKIPPER_MODES } from './introSkipperModes';
import * as styles from './shared.styles';
import { useOperationsConsoleStore } from './store';

interface IntroSkipCardProps {
  currentMode: IntroSkipperMode;
  onModeChange: (mode: IntroSkipperMode) => void;
}

export default function IntroSkipCard(props: IntroSkipCardProps) {
  const [ui] = useOperationsConsoleStore();

  return (
    <SectionCard icon={<Bot class={styles.sectionIcon.secondary} />} title="Intro Skip">
      <div class={styles.stack4}>
        <fieldset class={styles.fieldset} aria-label="Intro Skip Mode">
          <For each={INTRO_SKIPPER_MODES}>
            {(option) => (
              <button
                type="button"
                class={styles.choice}
                classList={{
                  [styles.choiceSelected]: props.currentMode === option.mode,
                  [styles.choiceIdle]: props.currentMode !== option.mode,
                }}
                aria-pressed={props.currentMode === option.mode}
                onClick={() => props.onModeChange(option.mode)}
              >
                <span class={styles.choiceTitle}>{option.label}</span>
                <span class={styles.choiceDescription}>{option.description}</span>
              </button>
            )}
          </For>
        </fieldset>
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
    </SectionCard>
  );
}
