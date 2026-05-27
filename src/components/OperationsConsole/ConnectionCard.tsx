import { Activity, Power, RefreshCw } from 'lucide-solid';
import { Show } from 'solid-js';
import type { ConnectionState } from '../../bindings';
import { SectionCard } from '../ui';
import { useOperationsConsoleStore } from './store';

interface ConnectionCardProps {
  state: ConnectionState | undefined;
  canReconnect: boolean;
  onDisconnect: () => void;
  onReconnect: () => void;
  onRefresh: () => void;
}

export default function ConnectionCard(props: ConnectionCardProps) {
  const [ui] = useOperationsConsoleStore();

  return (
    <SectionCard icon={<Activity class="h-6 w-6" />} title="Connection">
      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
        <div class="rounded-2xl bg-surface-container-high p-4">
          <p class="text-label-small uppercase text-on-surface-variant">
            Server
          </p>
          <p
            class="truncate text-title-medium text-on-surface"
            title={props.state?.serverName ?? ''}
          >
            {props.state?.serverName ?? 'Not connected'}
          </p>
        </div>
        <div class="rounded-2xl bg-surface-container-high p-4 md:col-span-2">
          <p class="text-label-small uppercase text-on-surface-variant">
            Server URL
          </p>
          <p
            class="truncate font-mono text-body-medium text-on-surface"
            title={props.state?.serverUrl ?? ''}
          >
            {props.state?.serverUrl ??
              'Reconnect with the Saved Session or sign in again'}
          </p>
        </div>
        <div class="rounded-2xl bg-surface-container-high p-4">
          <p class="text-label-small uppercase text-on-surface-variant">User</p>
          <p
            class="truncate text-title-medium text-on-surface"
            title={props.state?.userName ?? ''}
          >
            {props.state?.userName ?? 'No active user'}
          </p>
        </div>
      </div>
      <div class="mt-5 flex flex-wrap gap-3">
        <button
          type="button"
          class="btn-outlined"
          disabled={ui.disconnecting || !props.state?.connected}
          onClick={props.onDisconnect}
        >
          <Power class="h-5 w-5" />
          {ui.disconnecting ? 'Disconnecting...' : 'Disconnect'}
        </button>
        <Show when={!props.state?.connected && props.canReconnect}>
          <button
            type="button"
            class="btn-primary"
            disabled={ui.reconnecting}
            onClick={props.onReconnect}
          >
            {ui.reconnecting ? 'Reconnecting...' : 'Reconnect'}
          </button>
        </Show>
        <button
          type="button"
          onClick={props.onRefresh}
          class="btn-icon ml-auto"
          aria-label="Refresh status"
          title="Refresh status"
        >
          <RefreshCw class="h-5 w-5" />
        </button>
      </div>
      <p class="mt-3 text-body-small text-on-surface-variant">
        Disconnect ends the active Jellyfin connection but keeps the Saved
        Session available for Reconnect.
      </p>
    </SectionCard>
  );
}
