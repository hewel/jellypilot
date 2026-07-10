import { ToastProvider as UIToastProvider, useToast as useUIToast } from '@jellypilot/ui';
import type { ToastTone } from '@jellypilot/ui';
import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { createContext, onCleanup, onMount, useContext } from 'solid-js';
import type { ParentProps } from 'solid-js';

import type { NotificationLevel } from '../bindings';

/** Payload from backend AppNotification event. */
interface AppNotificationPayload {
  level: NotificationLevel;
  message: string;
}

interface ToastContextValue {
  showToast: (level: NotificationLevel, message: string) => void;
}

const ToastContext = createContext<ToastContextValue>();

const toastTones = {
  error: 'danger',
  info: 'info',
  success: 'success',
  warning: 'warning',
} satisfies Record<NotificationLevel, ToastTone>;

function AppToastAdapter(props: ParentProps) {
  const uiToast = useUIToast();
  let unlisten: UnlistenFn | undefined;

  const showToast = (level: NotificationLevel, message: string) => {
    uiToast.toast({
      title: message,
      tone: toastTones[level],
    });
  };

  onMount(async () => {
    try {
      unlisten = await listen<AppNotificationPayload>('app-notification', (event) => {
        showToast(event.payload.level, event.payload.message);
      });
    } catch (error) {
      console.error('Failed to listen for app notifications:', error);
    }
  });

  onCleanup(() => unlisten?.());

  return <ToastContext.Provider value={{ showToast }}>{props.children}</ToastContext.Provider>;
}

export function ToastProvider(props: ParentProps) {
  return (
    <UIToastProvider>
      <AppToastAdapter>{props.children}</AppToastAdapter>
    </UIToastProvider>
  );
}

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return context;
}
