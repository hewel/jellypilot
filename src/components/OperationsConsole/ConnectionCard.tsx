import { Button, IconButton } from '@jellypilot/ui';
import { Activity, AlertTriangle, Link, Power, RefreshCw, Server, User } from 'lucide-solid';
import { Show } from 'solid-js';

import type { ConnectionState } from '../../bindings';
import ConsoleSection from './ConsoleSection';
import { useOperationsConsoleStore } from './store';

import * as patterns from '../../styles/patterns.css';
import * as styles from './shared.css';

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
    <ConsoleSection icon={<Activity class={styles.sectionIcon.secondary} />} title="Connection">
      <div class={styles.grid3}>
        <div class={styles.tile}>
          <div class={styles.tileWatermark}>
            <Server class={styles.watermarkIcon} />
          </div>
          <p class={styles.overline}>Server</p>
          <p class={styles.value} title={props.state?.serverName ?? ''}>
            {props.state?.serverName ?? 'Not connected'}
          </p>
        </div>
        <div class={`${styles.tile} ${styles.span2}`}>
          <div class={styles.tileWatermark}>
            <Link class={styles.watermarkIcon} />
          </div>
          <p class={styles.overline}>Server URL</p>
          <p class={styles.monoValue} title={props.state?.serverUrl ?? ''}>
            {props.state?.serverUrl ?? 'Reconnect with a saved service or sign in again'}
          </p>
        </div>
        <div class={styles.tile}>
          <div class={styles.tileWatermark}>
            <User class={styles.watermarkIcon} />
          </div>
          <p class={styles.overline}>User</p>
          <p class={styles.value} title={props.state?.userName ?? ''}>
            {props.state?.userName ?? 'No active user'}
          </p>
        </div>
        <div class={`${styles.tile} ${styles.span2}`}>
          <div class={styles.tileWatermark}>
            <Activity class={styles.watermarkIcon} />
          </div>
          <p class={styles.overline}>Remote Control</p>
          <p class={styles.value}>{remoteControlLabel()}</p>
          <Show when={capabilities()?.remoteControlWarning}>
            {(message) => (
              <p class={styles.warning}>
                <AlertTriangle class={styles.warningIcon} />
                <span>{message()}</span>
              </p>
            )}
          </Show>
        </div>
      </div>

      <div class={styles.actionRow}>
        <Button
          type="button"
          variant="outline"
          class={styles.mutedOutlinedButton}
          disabled={ui.disconnecting || !props.state?.connected}
          onClick={props.onDisconnect}
        >
          <Power class={patterns.icon4_5} />
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
        <IconButton
          variant="outline"
          onClick={props.onRefresh}
          class={styles.refreshButton}
          aria-label="Refresh status"
        >
          <RefreshCw class={patterns.icon4_5} />
        </IconButton>
      </div>
      <p class={styles.bodyText}>
        Disconnect ends the active media server connection but keeps saved services available for
        Reconnect.
      </p>
    </ConsoleSection>
  );
}
