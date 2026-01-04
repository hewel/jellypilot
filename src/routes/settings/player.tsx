import { createFileRoute } from '@tanstack/solid-router';
import { Play } from 'lucide-solid';
import { Show } from 'solid-js';
import { css, cx } from '../../../styled-system/css';
import { button, input } from '../../../styled-system/recipes';
import { SectionCard, StatusBadge } from '../../components/ui';
import { useSettings } from './_context';

export const Route = createFileRoute('/settings/player')({
  component: PlayerSettings,
});

function PlayerSettings() {
  const { form, mpvConnected, detectingMpv, handleDetectMpv } = useSettings();

  return (
    <SectionCard
      icon={<Play class={css({ width: '24px', height: '24px' })} />}
      title="MPV Player"
      trailing={
        <Show when={!mpvConnected.loading}>
          <StatusBadge variant={mpvConnected() ? 'success' : 'neutral'}>
            {mpvConnected() ? 'Running' : 'Not Started'}
          </StatusBadge>
        </Show>
      }
    >
      <div
        class={css({
          display: 'flex',
          flexDirection: 'column',
          gap: '24px',
        })}
      >
        <form.Field name="mpvPath">
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
                MPV Executable Path
              </label>
              <div
                class={css({
                  display: 'flex',
                  gap: '8px',
                  alignItems: 'flex-start',
                })}
              >
                <input
                  id={field().name}
                  name={field().name}
                  type="text"
                  value={field().state.value}
                  onInput={(e) => field().handleChange(e.currentTarget.value)}
                  onBlur={() => field().handleBlur()}
                  placeholder="Path to mpv.exe or mpv binary"
                  class={cx(
                    input({ variant: 'filled' }),
                    css({ flex: 1, minWidth: 0 }),
                  )}
                />
                <button
                  type="button"
                  onClick={handleDetectMpv}
                  disabled={detectingMpv()}
                  class={cx(
                    button({ variant: 'tonal' }),
                    css({ height: '56px' }),
                  )}
                >
                  {detectingMpv() ? '...' : 'Auto-detect'}
                </button>
              </div>
            </div>
          )}
        </form.Field>

        <form.Field name="mpvArgs">
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
                Extra Arguments (one per line)
              </label>
              <textarea
                id={field().name}
                name={field().name}
                value={field().state.value}
                onInput={(e) => field().handleChange(e.currentTarget.value)}
                onBlur={() => field().handleBlur()}
                rows={4}
                placeholder="--fullscreen&#10;--force-window"
                class={css({
                  width: '100%',
                  backgroundColor: 'surfaceContainerHighest',
                  borderTopLeftRadius: '8px',
                  borderTopRightRadius: '8px',
                  borderBottomWidth: '1px',
                  borderBottomStyle: 'solid',
                  borderBottomColor: 'onSurfaceVariant',
                  paddingX: '16px',
                  paddingY: '12px',
                  color: 'onSurface',
                  fontFamily: 'mono',
                  textStyle: 'bodySmall',
                  lineHeight: 'relaxed',
                  outline: 'none',
                  transition: 'colors',
                  _placeholder: { color: 'onSurfaceVariant' },
                  _focus: {
                    borderBottomWidth: '2px',
                    borderBottomColor: 'primary',
                  },
                })}
              />
            </div>
          )}
        </form.Field>
      </div>
    </SectionCard>
  );
}
