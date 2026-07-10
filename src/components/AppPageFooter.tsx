import AppVersion from './AppVersion';

import * as styles from './AppPageFooter.css';

interface PageFooterProps {
  appName?: string;
  class?: string;
}

/**
 * Consistent page footer with app name and version.
 */
export default function PageFooter(props: PageFooterProps) {
  return (
    <div class={`${styles.root} ${props.class ?? ''}`}>
      <p class={styles.text}>{props.appName ?? 'JellyPilot'}</p>
      <AppVersion />
    </div>
  );
}
