import { createFileRoute } from '@tanstack/solid-router';
import { Cast } from 'lucide-solid';
import { Show } from 'solid-js';
import { css } from '../../../styled-system/css';
import { input } from '../../../styled-system/recipes';
import { SectionCard } from '../../components/ui';
import { useSettings } from './_context';

export const Route = createFileRoute('/settings/device')({
  component: DeviceSettings,
});

function DeviceSettings() {
  const { form } = useSettings();

  return (
    <SectionCard
      icon={<Cast class={css({ width: '24px', height: '24px' })} />}
      title="Device Settings"
    >
      <div
        class={css({
          display: 'flex',
          flexDirection: 'column',
          gap: '16px',
        })}
      >
        <form.Field
          name="deviceName"
          validators={{
            onChange: ({ value }: { value: string }) =>
              !value.trim() ? 'Device name is required' : undefined,
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
                Device Name
              </label>
              <input
                id={field().name}
                name={field().name}
                type="text"
                value={field().state.value}
                onInput={(e) => field().handleChange(e.currentTarget.value)}
                onBlur={() => field().handleBlur()}
                class={input({ variant: 'filled' })}
                placeholder="JMSR"
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
              <p
                class={css({
                  color: 'onSurfaceVariant/70',
                  textStyle: 'bodySmall',
                  marginTop: '6px',
                  marginLeft: '4px',
                })}
              >
                Name displayed in Jellyfin cast menu
              </p>
            </div>
          )}
        </form.Field>
      </div>
    </SectionCard>
  );
}
