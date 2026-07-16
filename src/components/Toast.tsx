import { Match } from 'effect';
import { Check, Info, TriangleAlert, X } from 'lucide-solid';
import { onCleanup, onMount } from 'solid-js';
import type { JSX } from 'solid-js';

import type { NotificationLevel } from '../bindings';
import * as styles from './Toast.styles';

export type { NotificationLevel };

interface ToastProps {
  id: string;
  level: NotificationLevel;
  message: string;
  exiting: boolean;
  onDismiss: (id: string) => void;
}
const toastIcon = Match.type<NotificationLevel>().pipe(
  Match.withReturnType<JSX.Element>(),
  Match.when('success', () => <Check class={styles.icon({ level: 'success' })} />),
  Match.when('error', () => <X class={styles.icon({ level: 'error' })} />),
  Match.when('warning', () => <TriangleAlert class={styles.icon({ level: 'warning' })} />),
  Match.when('info', () => <Info class={styles.icon({ level: 'info' })} />),
  Match.exhaustive,
);

export default function Toast(props: ToastProps) {
  let timer: ReturnType<typeof setTimeout>;

  onMount(() => {
    timer = setTimeout(() => {
      props.onDismiss(props.id);
    }, 5000);
  });

  onCleanup(() => {
    if (timer) {
      clearTimeout(timer);
    }
  });

  const getIcon = () => toastIcon(props.level);

  return (
    <div
      class={styles.toast({ level: props.level })}
      classList={{ [styles.exiting]: props.exiting, [styles.visible]: !props.exiting }}
      role="alert"
    >
      <div class={styles.iconWrap}>{getIcon()}</div>
      <div class={styles.message}>{props.message}</div>
      <button
        type="button"
        class={styles.closeButton}
        onClick={() => props.onDismiss(props.id)}
        aria-label="Close"
      >
        <span class={styles.srOnly}>Close</span>
        <X class={styles.closeIcon} />
      </button>
    </div>
  );
}
