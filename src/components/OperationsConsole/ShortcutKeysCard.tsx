import { Field as ArkField } from '@ark-ui/solid/field';
import { Keyboard } from 'lucide-solid';
import { Show } from 'solid-js';

import { FieldControl, SectionCard } from '../ui';
import * as shared from './shared.styles';
import * as styles from './ShortcutKeysCard.styles';
import type { OperationsConsoleForm } from './types';

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
            <ArkField.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class={shared.overline}>Next episode key</ArkField.Label>
              <ArkField.Input
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
            </ArkField.Root>
          )}
        </props.form.Field>

        <props.form.Field
          name="keybindPrev"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
          }}
        >
          {(field) => (
            <ArkField.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class={shared.overline}>Previous episode key</ArkField.Label>
              <ArkField.Input
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
            </ArkField.Root>
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
              <ArkField.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
                <ArkField.Label class={shared.overline}>Intro skip key</ArkField.Label>
                <ArkField.Input
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
              </ArkField.Root>
            )}
          </props.form.Field>
        </Show>
      </div>
    </SectionCard>
  );
}
