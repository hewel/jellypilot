import { Bot } from 'lucide-solid';
import { For, Show } from 'solid-js';
import type { IntroSkipperMode } from '../../bindings';
import { SectionCard } from '../ui';
import { INTRO_SKIPPER_MODES } from './introSkipperModes';
import { useOperationsConsoleStore } from './store';

interface IntroSkipCardProps {
  currentMode: IntroSkipperMode;
  onModeChange: (mode: IntroSkipperMode) => void;
}

export default function IntroSkipCard(props: IntroSkipCardProps) {
  const [ui] = useOperationsConsoleStore();

  return (
    <SectionCard icon={<Bot class="h-6 w-6" />} title="Intro Skip">
      <div class="space-y-4">
        <fieldset class="grid grid-cols-1 gap-3" aria-label="Intro Skip Mode">
          <For each={INTRO_SKIPPER_MODES}>
            {(option) => (
              <button
                type="button"
                class={`rounded-2xl border px-4 py-3 text-left transition ${
                  props.currentMode === option.mode
                    ? 'border-primary bg-primary-container text-on-primary-container'
                    : 'border-outline-variant bg-surface-container-high text-on-surface hover:border-primary/50'
                }`}
                aria-pressed={props.currentMode === option.mode}
                onClick={() => props.onModeChange(option.mode)}
              >
                <span class="block text-title-medium">{option.label}</span>
                <span class="mt-1 block text-body-small opacity-80">
                  {option.description}
                </span>
              </button>
            )}
          </For>
        </fieldset>
        <Show when={ui.introSkipperSaving}>
          <p class="text-body-small text-secondary">Saving preference…</p>
        </Show>
        <p class="text-body-small text-on-surface-variant">
          Changes take effect after restarting MPV.
        </p>
        <Show when={ui.introSkipperError}>
          {(message) => (
            <p class="rounded-2xl bg-error-container px-4 py-3 text-body-small text-on-error-container">
              {message()}
            </p>
          )}
        </Show>
      </div>
    </SectionCard>
  );
}
