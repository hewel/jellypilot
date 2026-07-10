import { type ParentProps, Show } from 'solid-js';

import { ConfigCoordinatorProvider, useConfigCoordinator } from '../effects/configContext';

const bootStyle = {
  minHeight: '100vh',
  display: 'grid',
  placeItems: 'center',
  padding: '1.5rem',
  background: '#0b0d14',
  color: '#f3f6ff',
  fontFamily: 'system-ui, sans-serif',
} as const;

function ConfigGateInner(props: ParentProps) {
  const { coordinator, state } = useConfigCoordinator();

  return (
    <Show
      when={state().status === 'ready' || state().status === 'saving'}
      fallback={
        <div style={bootStyle} data-theme="system" data-testid="config-boot">
          <div>
            <p>
              {state().status === 'loading'
                ? 'Loading configuration…'
                : (state().error?.message ?? 'Configuration failed to load.')}
            </p>
            <Show when={state().status === 'error'}>
              <button type="button" onClick={() => void coordinator.retry()}>
                Retry
              </button>
            </Show>
          </div>
        </div>
      }
    >
      <div
        data-theme={
          state().desired?.themePreference === 'system'
            ? undefined
            : state().desired?.themePreference
        }
        data-theme-preference={state().desired?.themePreference ?? 'system'}
      >
        {props.children}
      </div>
    </Show>
  );
}

export function ConfigGate(props: ParentProps) {
  return (
    <ConfigCoordinatorProvider>
      <ConfigGateInner>{props.children}</ConfigGateInner>
    </ConfigCoordinatorProvider>
  );
}
