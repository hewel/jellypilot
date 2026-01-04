import { createFileRoute } from '@tanstack/solid-router';
import { Keyboard } from 'lucide-solid';
import { Show } from 'solid-js';
import { css, cx } from '../../../styled-system/css';
import { input } from '../../../styled-system/recipes';
import { SectionCard } from '../../components/ui';
import { useSettings } from './_context';

export const Route = createFileRoute('/settings/keybindings')({
  component: KeybindingsSettings,
});

function KeybindingsSettings() {
  const { form } = useSettings();

  return (
    <SectionCard
      icon={<Keyboard class={css({ width: '24px', height: '24px' })} />}
      title="Keybindings"
    >
      <p
        class={css({
          color: 'onSurfaceVariant/80',
          textStyle: 'bodyMedium',
          marginBottom: '24px',
          marginTop: '-16px',
          marginLeft: '36px',
        })}
      >
        Keyboard shortcuts for MPV episode navigation. Changes take effect on
        next MPV restart.
      </p>

      <div
        class={css({
          display: 'grid',
          gridTemplateColumns: '1fr',
          md: { gridTemplateColumns: 'repeat(2, 1fr)' },
          gap: '24px',
        })}
      >
        <form.Field
          name="keybindNext"
          validators={{
            onChange: ({ value }: { value: string }) =>
              !value.trim() ? 'Keybinding is required' : undefined,
          }}
        >
          {(field: any) => (
            <div
              class={css({
                _focusWithin: { '& label': { color: 'primary' } },
              })}
            >
              <label
                for={field().name}
                class={css({
                  display: 'block',
                  textStyle: 'labelSmall',
                  color: 'onSurfaceVariant',
                  marginBottom: '6px',
                  marginLeft: '4px',
                  textTransform: 'uppercase',
                  letterSpacing: 'wider',
                  transition: 'colors',
                })}
              >
                Next Episode
              </label>
              <input
                id={field().name}
                name={field().name}
                type="text"
                value={field().state.value}
                onInput={(e) => field().handleChange(e.currentTarget.value)}
                onBlur={() => field().handleBlur()}
                class={cx(
                  input({ variant: 'filled' }),
                  css({ fontFamily: 'mono', textAlign: 'center' }),
                )}
                placeholder="Shift+n"
              />
              <Show when={field().state.meta.errors.length > 0}>
                <p
                  class={css({
                    color: 'error',
                    textStyle: 'bodySmall',
                    marginTop: '6px',
                    marginLeft: '4px',
                  })}
                >
                  {field().state.meta.errors[0]}
                </p>
              </Show>
            </div>
          )}
        </form.Field>

        <form.Field
          name="keybindPrev"
          validators={{
            onChange: ({ value }: { value: string }) =>
              !value.trim() ? 'Keybinding is required' : undefined,
          }}
        >
          {(field: any) => (
            <div
              class={css({
                _focusWithin: { '& label': { color: 'primary' } },
              })}
            >
              <label
                for={field().name}
                class={css({
                  display: 'block',
                  textStyle: 'labelSmall',
                  color: 'onSurfaceVariant',
                  marginBottom: '6px',
                  marginLeft: '4px',
                  textTransform: 'uppercase',
                  letterSpacing: 'wider',
                  transition: 'colors',
                })}
              >
                Previous Episode
              </label>
              <input
                id={field().name}
                name={field().name}
                type="text"
                value={field().state.value}
                onInput={(e) => field().handleChange(e.currentTarget.value)}
                onBlur={() => field().handleBlur()}
                class={cx(
                  input({ variant: 'filled' }),
                  css({ fontFamily: 'mono', textAlign: 'center' }),
                )}
                placeholder="Shift+p"
              />
              <Show when={field().state.meta.errors.length > 0}>
                <p
                  class={css({
                    color: 'error',
                    textStyle: 'bodySmall',
                    marginTop: '6px',
                    marginLeft: '4px',
                  })}
                >
                  {field().state.meta.errors[0]}
                </p>
              </Show>
            </div>
          )}
        </form.Field>
      </div>

      <p
        class={css({
          color: 'onSurfaceVariant/60',
          textStyle: 'bodySmall',
          marginTop: '24px',
          textAlign: 'center',
          borderTopWidth: '1px',
          borderTopStyle: 'solid',
          borderTopColor: 'outlineVariant/20',
          paddingTop: '16px',
        })}
      >
        Use MPV keybinding syntax (e.g., Shift+n, Ctrl+Left, Alt+q)
      </p>
    </SectionCard>
  );
}
