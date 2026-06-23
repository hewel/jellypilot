import { Activity, AlertTriangle, Link, Power, RefreshCw, Server, User } from 'lucide-solid';
import { Show } from 'solid-js';

import type { ConnectionState } from '../../bindings';
import { Button, SectionCard } from '../ui';
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
  const capabilities = () => props.state?.capabilities;
  const remoteControlLabel = () => {
    const caps = capabilities();
    if (!caps?.remoteControl) return 'Unavailable';
    return caps.remoteControlAvailable ? 'Available' : 'Pending';
  };

  return (
    <SectionCard
      icon={<Activity class="text-secondary h-5 w-5 drop-shadow-[0_0_8px_rgba(129,140,248,0.4)]" />}
      title="Connection"
    >
      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
        <div class="bg-surface-container-high/30 border-outline-variant/60 relative overflow-hidden rounded-2xl border p-4 backdrop-blur-sm">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <Server class="h-12 w-12" />
          </div>
          <p class="text-on-surface-variant text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
            Server
          </p>
          <p
            class="text-on-surface mt-1.5 truncate text-[16px] leading-[24px] font-bold font-semibold"
            title={props.state?.serverName ?? ''}
          >
            {props.state?.serverName ?? 'Not connected'}
          </p>
        </div>
        <div class="bg-surface-container-high/30 border-outline-variant/60 relative overflow-hidden rounded-2xl border p-4 backdrop-blur-sm md:col-span-2">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <Link class="h-12 w-12" />
          </div>
          <p class="text-on-surface-variant text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
            Server URL
          </p>
          <p
            class="text-secondary mt-1.5 truncate font-mono text-[14px] leading-[20px]"
            title={props.state?.serverUrl ?? ''}
          >
            {props.state?.serverUrl ?? 'Reconnect with a saved service or sign in again'}
          </p>
        </div>
        <div class="bg-surface-container-high/30 border-outline-variant/60 relative overflow-hidden rounded-2xl border p-4 backdrop-blur-sm">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <User class="h-12 w-12" />
          </div>
          <p class="text-on-surface-variant text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
            User
          </p>
          <p
            class="text-on-surface mt-1.5 truncate text-[16px] leading-[24px] font-bold font-semibold"
            title={props.state?.userName ?? ''}
          >
            {props.state?.userName ?? 'No active user'}
          </p>
        </div>
        <div class="bg-surface-container-high/30 border-outline-variant/60 relative overflow-hidden rounded-2xl border p-4 backdrop-blur-sm md:col-span-2">
          <div class="absolute top-0 right-0 p-3 opacity-5">
            <Activity class="h-12 w-12" />
          </div>
          <p class="text-on-surface-variant text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
            Remote Control
          </p>
          <p class="text-on-surface mt-1.5 text-[16px] leading-[24px] font-bold font-semibold">
            {remoteControlLabel()}
          </p>
          <Show when={capabilities()?.remoteControlWarning}>
            {(message) => (
              <p class="text-warning mt-2 flex items-start gap-2 text-[12px] leading-[16px] font-semibold">
                <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>{message()}</span>
              </p>
            )}
          </Show>
        </div>
      </div>

      <div class="mt-6 flex flex-wrap items-center gap-3">
        <Button
          type="button"
          variant="outlined"
          class="text-on-surface-variant hover:border-primary/50 hover:text-on-surface"
          disabled={ui.disconnecting || !props.state?.connected}
          onClick={props.onDisconnect}
          leadingIcon={<Power class="h-4.5 w-4.5" />}
        >
          {ui.disconnecting ? 'Disconnecting...' : 'Disconnect'}
        </Button>
        <Show when={!props.state?.connected && props.canReconnect}>
          <Button
            type="button"
            variant="primary"
            disabled={ui.reconnecting}
            onClick={props.onReconnect}
          >
            {ui.reconnecting ? 'Reconnecting...' : 'Reconnect'}
          </Button>
        </Show>
        <Button
          type="button"
          variant="icon"
          onClick={props.onRefresh}
          class="border-outline-variant bg-surface-container-high/20 hover:border-secondary hover:text-secondary ml-auto rounded-xl border"
          aria-label="Refresh status"
          title="Refresh status"
        >
          <RefreshCw class="h-4.5 w-4.5" />
        </Button>
      </div>
      <p class="text-on-surface-variant/80 mt-4 text-[12px] leading-[16px]">
        Disconnect ends the active media server connection but keeps saved services available for
        Reconnect.
      </p>
    </SectionCard>
  );
}
