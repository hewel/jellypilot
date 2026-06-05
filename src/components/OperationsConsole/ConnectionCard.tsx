import { Activity, Link, Power, RefreshCw, Server, User } from 'lucide-solid';
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
    <SectionCard
      icon={
        <Activity class="h-5 w-5 text-secondary drop-shadow-[0_0_8px_rgba(129,140,248,0.4)]" />
      }
      title="Connection"
    >
      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
        <div class="rounded-2xl bg-surface-container-high/30 p-4 border border-outline-variant/60 relative overflow-hidden backdrop-blur-sm">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <Server class="h-12 w-12" />
          </div>
          <p class="text-label-small uppercase text-on-surface-variant">
            Server
          </p>
          <p
            class="truncate text-title-medium text-on-surface mt-1.5 font-bold"
            title={props.state?.serverName ?? ''}
          >
            {props.state?.serverName ?? 'Not connected'}
          </p>
        </div>
        <div class="rounded-2xl bg-surface-container-high/30 p-4 md:col-span-2 border border-outline-variant/60 relative overflow-hidden backdrop-blur-sm">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <Link class="h-12 w-12" />
          </div>
          <p class="text-label-small uppercase text-on-surface-variant">
            Server URL
          </p>
          <p
            class="truncate font-mono text-body-medium text-secondary mt-1.5"
            title={props.state?.serverUrl ?? ''}
          >
            {props.state?.serverUrl ??
              'Reconnect with the Saved Session or sign in again'}
          </p>
        </div>
        <div class="rounded-2xl bg-surface-container-high/30 p-4 border border-outline-variant/60 relative overflow-hidden backdrop-blur-sm">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <User class="h-12 w-12" />
          </div>
          <p class="text-label-small uppercase text-on-surface-variant">User</p>
          <p
            class="truncate text-title-medium text-on-surface mt-1.5 font-bold"
            title={props.state?.userName ?? ''}
          >
            {props.state?.userName ?? 'No active user'}
          </p>
        </div>
      </div>

      <div class="mt-6 flex flex-wrap items-center gap-3">
        <button
          type="button"
          class="btn-outlined text-on-surface-variant hover:border-primary/50 hover:text-on-surface"
          disabled={ui.disconnecting || !props.state?.connected}
          onClick={props.onDisconnect}
        >
          <Power class="h-4.5 w-4.5" />
          <span>{ui.disconnecting ? 'Disconnecting...' : 'Disconnect'}</span>
        </button>
        <Show when={!props.state?.connected && props.canReconnect}>
          <button
            type="button"
            class="btn-primary"
            disabled={ui.reconnecting}
            onClick={props.onReconnect}
          >
            <span>{ui.reconnecting ? 'Reconnecting...' : 'Reconnect'}</span>
          </button>
        </Show>
        <button
          type="button"
          onClick={props.onRefresh}
          class="btn-icon ml-auto rounded-xl border border-outline-variant bg-surface-container-high/20 hover:border-secondary hover:text-secondary"
          aria-label="Refresh status"
          title="Refresh status"
        >
          <RefreshCw class="h-4.5 w-4.5" />
        </button>
      </div>
      <p class="mt-4 text-body-small text-on-surface-variant/80">
        Disconnect ends the active Jellyfin connection but keeps the Saved
        Session available for Reconnect.
      </p>
    </SectionCard>
  );
}
