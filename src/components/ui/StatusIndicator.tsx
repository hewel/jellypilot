import * as styles from './StatusIndicator.styles';

interface StatusIndicatorProps {
  connected: boolean;
  connectedText?: string;
  disconnectedText?: string;
}

/**
 * Status indicator with glowing dot and text.
 */
export default function StatusIndicator(props: StatusIndicatorProps) {
  const connectedText = () => props.connectedText ?? 'Connected';
  const disconnectedText = () => props.disconnectedText ?? 'Disconnected';

  return (
    <div class={styles.root}>
      <span class={styles.statusDot({ connected: props.connected })} />
      <span class={styles.text({ connected: props.connected })}>
        {props.connected ? connectedText() : disconnectedText()}
      </span>
    </div>
  );
}
