import { createFileRoute } from '@tanstack/solid-router';
import { CircleCheckBig } from 'lucide-solid';
import { Show } from 'solid-js';
import { css } from '../../../styled-system/css';
import { InfoCard, SectionCard, StatusIndicator } from '../../components/ui';
import { useSettings } from './_context';

export const Route = createFileRoute('/settings/')({
  component: ConnectionSettings,
});

function ConnectionSettings() {
  const { connectionState } = useSettings();
  const state = () => connectionState();

  return (
    <SectionCard
      icon={<CircleCheckBig class={css({ width: '24px', height: '24px' })} />}
      title="Jellyfin Connection"
    >
      <Show
        when={!connectionState.loading}
        fallback={
          <div
            class={css({
              animation: 'pulse 2s infinite',
              height: '96px',
              backgroundColor: 'surfaceContainerHigh',
              borderRadius: '12px',
            })}
          />
        }
      >
        <div
          class={css({
            display: 'grid',
            gridTemplateColumns: '1fr',
            md: { gridTemplateColumns: 'repeat(2, 1fr)' },
            gap: '16px',
          })}
        >
          <InfoCard label="Status">
            <StatusIndicator connected={state()?.connected ?? false} />
          </InfoCard>

          <Show when={state()?.serverName}>
            <InfoCard label="Server">
              <span
                class={css({
                  color: 'onSurface',
                  fontWeight: 'medium',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  display: 'block',
                })}
                title={state()?.serverName ?? ''}
              >
                {state()?.serverName}
              </span>
            </InfoCard>
          </Show>

          <Show when={state()?.serverUrl}>
            <InfoCard label="URL">
              <span
                class={css({
                  color: 'onSurface',
                  fontWeight: 'medium',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  display: 'block',
                })}
                title={state()?.serverUrl ?? ''}
              >
                {state()?.serverUrl}
              </span>
            </InfoCard>
          </Show>

          <Show when={state()?.userName}>
            <InfoCard label="User">
              <span
                class={css({
                  color: 'onSurface',
                  fontWeight: 'medium',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  display: 'block',
                })}
                title={state()?.userName ?? ''}
              >
                {state()?.userName}
              </span>
            </InfoCard>
          </Show>
        </div>
      </Show>
    </SectionCard>
  );
}
