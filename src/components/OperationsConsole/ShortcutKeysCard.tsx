import { Field as ArkField } from '@ark-ui/solid/field';
import { Keyboard } from 'lucide-solid';

import { FieldControl, SectionCard } from '../ui';
import type { OperationsConsoleForm } from './types';

interface ShortcutKeysCardProps {
  form: OperationsConsoleForm;
  onSaveTextSetting: (
    field: 'keybindNext' | 'keybindPrev' | 'keybindIntroSkip',
    value: string,
  ) => void;
}

export default function ShortcutKeysCard(props: ShortcutKeysCardProps) {
  return (
    <SectionCard
      icon={<Keyboard class="text-secondary h-5 w-5 drop-shadow-[0_0_8px_rgba(129,140,248,0.4)]" />}
      title="Shortcut keys"
    >
      <div class="space-y-4">
        <p class="text-on-surface-variant/80 text-[12px] leading-[16px]">
          MPV input bindings for episode navigation and manual intro skipping.
        </p>

        <props.form.Field
          name="keybindNext"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
          }}
        >
          {(field) => (
            <ArkField.Root class="block" invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                Next episode key
              </ArkField.Label>
              <ArkField.Input
                asChild={(fieldProps) => (
                  <FieldControl
                    {...fieldProps()}
                    variant="filled"
                    id={field().name}
                    name={field().name}
                    type="text"
                    value={field().state.value}
                    onInput={(event) => field().handleChange(event.currentTarget.value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('keybindNext', event.currentTarget.value);
                    }}
                    class="text-secondary w-full font-mono font-semibold"
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
            <ArkField.Root class="block" invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                Previous episode key
              </ArkField.Label>
              <ArkField.Input
                asChild={(fieldProps) => (
                  <FieldControl
                    {...fieldProps()}
                    variant="filled"
                    id={field().name}
                    name={field().name}
                    type="text"
                    value={field().state.value}
                    onInput={(event) => field().handleChange(event.currentTarget.value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('keybindPrev', event.currentTarget.value);
                    }}
                    class="text-secondary w-full font-mono font-semibold"
                    placeholder="Shift+<"
                  />
                )}
              />
            </ArkField.Root>
          )}
        </props.form.Field>

        <props.form.Field
          name="keybindIntroSkip"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Keybinding is required' : undefined),
          }}
        >
          {(field) => (
            <ArkField.Root class="block" invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                Intro skip key
              </ArkField.Label>
              <ArkField.Input
                asChild={(fieldProps) => (
                  <FieldControl
                    {...fieldProps()}
                    variant="filled"
                    id={field().name}
                    name={field().name}
                    type="text"
                    value={field().state.value}
                    onInput={(event) => field().handleChange(event.currentTarget.value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('keybindIntroSkip', event.currentTarget.value);
                    }}
                    class="text-secondary w-full font-mono font-semibold"
                    placeholder="g"
                  />
                )}
              />
            </ArkField.Root>
          )}
        </props.form.Field>
      </div>
    </SectionCard>
  );
}
