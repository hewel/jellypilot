import { TextInput } from '@jellypilot/ui';
import { Keyboard } from 'lucide-solid';
import { Show } from 'solid-js';

import ConsoleSection from './ConsoleSection';
import type { OperationsConsoleForm } from './types';

import * as shared from './shared.css';
import * as styles from './ShortcutKeysCard.css';

interface ShortcutKeysCardProps {
  form: OperationsConsoleForm;
  showIntroSkipKey: boolean;
  onSaveTextSetting: (
    field: 'keybindNext' | 'keybindPrev' | 'keybindIntroSkip',
    value: string,
  ) => void;
}

export default function ShortcutKeysCard(props: ShortcutKeysCardProps) {
  return (
    <ConsoleSection icon={<Keyboard class={shared.sectionIcon.secondary} />} title="Shortcut keys">
      <div class={shared.stack4}>
        <p class={styles.description}>
          {props.showIntroSkipKey
            ? 'MPV input bindings for episode navigation and manual intro skipping.'
            : 'MPV input bindings for episode navigation.'}
        </p>

        <props.form.Field
          name="keybindNext"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
          }}
        >
          {(field) => (
            <TextInput
              name={field().name}
              label="Next episode key"
              error={field().state.meta.errors[0]}
              value={field().state.value}
              onValueChange={(value) => field().handleChange(value)}
              onBlur={(event) => {
                field().handleBlur();
                props.onSaveTextSetting('keybindNext', event.currentTarget.value);
              }}
              class={styles.input}
              placeholder="Shift+>"
            />
          )}
        </props.form.Field>

        <props.form.Field
          name="keybindPrev"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
          }}
        >
          {(field) => (
            <TextInput
              name={field().name}
              label="Previous episode key"
              error={field().state.meta.errors[0]}
              value={field().state.value}
              onValueChange={(value) => field().handleChange(value)}
              onBlur={(event) => {
                field().handleBlur();
                props.onSaveTextSetting('keybindPrev', event.currentTarget.value);
              }}
              class={styles.input}
              placeholder="Shift+<"
            />
          )}
        </props.form.Field>

        <Show when={props.showIntroSkipKey}>
          <props.form.Field
            name="keybindIntroSkip"
            validators={{
              onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
            }}
          >
            {(field) => (
              <TextInput
                name={field().name}
                label="Intro skip key"
                error={field().state.meta.errors[0]}
                value={field().state.value}
                onValueChange={(value) => field().handleChange(value)}
                onBlur={(event) => {
                  field().handleBlur();
                  props.onSaveTextSetting('keybindIntroSkip', event.currentTarget.value);
                }}
                class={styles.input}
                placeholder="g"
              />
            )}
          </props.form.Field>
        </Show>
      </div>
    </ConsoleSection>
  );
}
