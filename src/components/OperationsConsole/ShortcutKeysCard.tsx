import { Keyboard } from 'lucide-solid';
import { Show } from 'solid-js';

import { Field, FieldControl, SectionCard } from '../ui';
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
    <SectionCard icon={<Keyboard class={shared.sectionIcon.secondary} />} title="Shortcut keys">
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
            <Field.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
              <Field.Label class={shared.overline}>Next episode key</Field.Label>
              <Field.Input
                asChild={(fieldProps) => (
                  <FieldControl
                    {...fieldProps()}
                    variant="filled"
                    name={field().name}
                    type="text"
                    value={field().state.value}
                    onInput={(event) => field().handleChange(event.currentTarget.value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('keybindNext', event.currentTarget.value);
                    }}
                    class={styles.input}
                    placeholder="Shift+>"
                  />
                )}
              />
            </Field.Root>
          )}
        </props.form.Field>

        <props.form.Field
          name="keybindPrev"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
          }}
        >
          {(field) => (
            <Field.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
              <Field.Label class={shared.overline}>Previous episode key</Field.Label>
              <Field.Input
                asChild={(fieldProps) => (
                  <FieldControl
                    {...fieldProps()}
                    variant="filled"
                    name={field().name}
                    type="text"
                    value={field().state.value}
                    onInput={(event) => field().handleChange(event.currentTarget.value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('keybindPrev', event.currentTarget.value);
                    }}
                    class={styles.input}
                    placeholder="Shift+<"
                  />
                )}
              />
            </Field.Root>
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
              <Field.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
                <Field.Label class={shared.overline}>Intro skip key</Field.Label>
                <Field.Input
                  asChild={(fieldProps) => (
                    <FieldControl
                      {...fieldProps()}
                      variant="filled"
                      name={field().name}
                      type="text"
                      value={field().state.value}
                      onInput={(event) => field().handleChange(event.currentTarget.value)}
                      onBlur={(event) => {
                        field().handleBlur();
                        props.onSaveTextSetting('keybindIntroSkip', event.currentTarget.value);
                      }}
                      class={styles.input}
                      placeholder="g"
                    />
                  )}
                />
              </Field.Root>
            )}
          </props.form.Field>
        </Show>
      </div>
    </SectionCard>
  );
}
