import { UIRoot, jellypilotTheme } from '@jellypilot/ui';
import type { UIRootProps } from '@jellypilot/ui';
import '@jellypilot/ui/theme/jellypilot-font';
import { type ParentProps, Show } from 'solid-js';

import { ConfigCoordinatorProvider, useConfigCoordinator } from '../effects/configContext';

const bootStyle = {
  minHeight: '100vh',
  display: 'grid',
  placeItems: 'center',
  padding: '1.5rem',
  background: 'var(--jellypilot-color-surface)',
  color: 'var(--jellypilot-color-on-surface)',
  fontFamily: 'var(--jellypilot-font-sans)',
} as const;

type ConfigGateProps = ParentProps<{
  linkAdapter?: UIRootProps['linkAdapter'];
}>;

function ConfigGateInner(props: ConfigGateProps) {
  const { coordinator, state } = useConfigCoordinator();

  return (
    <UIRoot
      preference={state().desired?.themePreference ?? 'system'}
      theme={jellypilotTheme}
      linkAdapter={props.linkAdapter}
    >
      <Show
        when={state().status === 'ready' || state().status === 'saving'}
        fallback={
          <div style={bootStyle} data-testid="config-boot">
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
        <div data-theme-preference={state().desired?.themePreference ?? 'system'}>
          {props.children}
        </div>
      </Show>
    </UIRoot>
  );
}

export function ConfigGate(props: ConfigGateProps) {
  return (
    <ConfigCoordinatorProvider>
      <ConfigGateInner linkAdapter={props.linkAdapter}>{props.children}</ConfigGateInner>
    </ConfigCoordinatorProvider>
  );
}
