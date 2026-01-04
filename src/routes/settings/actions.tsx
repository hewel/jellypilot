import { createFileRoute } from '@tanstack/solid-router';
import { LogOut, ShieldAlert, Trash2 } from 'lucide-solid';
import { css, cx } from '../../../styled-system/css';
import { button, card } from '../../../styled-system/recipes';
import { useSettings } from './_context';

export const Route = createFileRoute('/settings/actions')({
  component: ActionsSettings,
});

function ActionsSettings() {
  const {
    connectionState,
    disconnecting,
    clearingSession,
    handleDisconnect,
    handleClearSession,
  } = useSettings();

  const state = () => connectionState();

  return (
    <div
      class={cx(
        card({ variant: 'filled' }),
        css({ position: 'relative', overflow: 'hidden' }),
      )}
    >
      <div
        class={css({
          position: 'absolute',
          inset: 0,
          backgroundColor: 'primary/3',
          pointerEvents: 'none',
        })}
      />
      <div class={css({ position: 'relative', zIndex: 10 })}>
        <h2
          class={css({
            textStyle: 'titleMedium',
            color: 'onSurface',
            marginBottom: '24px',
            display: 'flex',
            alignItems: 'center',
            gap: '12px',
          })}
        >
          <ShieldAlert class={css({ width: '24px', height: '24px' })} />
          Danger Zone
        </h2>

        <div
          class={css({
            display: 'grid',
            gridTemplateColumns: '1fr',
            md: { gridTemplateColumns: 'repeat(2, 1fr)' },
            gap: '16px',
          })}
        >
          <button
            type="button"
            onClick={handleDisconnect}
            disabled={disconnecting() || !state()?.connected}
            class={cx(
              button({ variant: 'outlined' }),
              css({
                borderColor: 'error/50',
                color: 'error',
                width: '100%',
                _hover: {
                  backgroundColor: 'error/10',
                  borderColor: 'error',
                },
              }),
            )}
          >
            <LogOut class={css({ width: '20px', height: '20px' })} />
            {disconnecting() ? 'Disconnecting...' : 'Disconnect'}
          </button>

          <button
            type="button"
            onClick={handleClearSession}
            disabled={clearingSession()}
            class={cx(button({ variant: 'tonal' }), css({ width: '100%' }))}
          >
            <Trash2 class={css({ width: '20px', height: '20px' })} />
            {clearingSession() ? 'Clearing...' : 'Clear Session'}
          </button>
        </div>
        <p
          class={css({
            color: 'onSurfaceVariant/60',
            textStyle: 'bodySmall',
            marginTop: '16px',
            textAlign: 'center',
          })}
        >
          Clear saved session will remove stored credentials and return to login
        </p>
      </div>
    </div>
  );
}
