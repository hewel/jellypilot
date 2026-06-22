import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { createContext, createSignal, For, onCleanup, onMount, useContext } from 'solid-js';
import type { ParentProps } from 'solid-js';

import Toast from './Toast';
import type { NotificationLevel } from './Toast';

const TOAST_EXIT_MS = 200;
export interface ToastMessage {
  id: string;
  level: NotificationLevel;
  message: string;
}

/** Payload from backend AppNotification event */
interface AppNotificationPayload {
  level: NotificationLevel;
  message: string;
}

interface ToastContextValue {
  showToast: (level: NotificationLevel, message: string) => void;
  removeToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue>();

export function ToastProvider(props: ParentProps) {
  const [toasts, setToasts] = createSignal<ToastMessage[]>([]);
  let unlisten: UnlistenFn | undefined;
  const [exitingToastIds, setExitingToastIds] = createSignal<Set<string>>(new Set());
  const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

  const showToast = (level: NotificationLevel, message: string) => {
    const id = Math.random().toString(36).slice(2, 9);
    setToasts((prev) => [...prev, { id, level, message }]);
  };

  const removeToast = (id: string) => {
    const present = toasts().some((toast) => toast.id === id);
    if (!present || exitingToastIds().has(id)) {
      return;
    }
    setExitingToastIds((current) => new Set(current).add(id));
    dismissTimers.set(
      id,
      setTimeout(() => {
        setToasts((prev) => prev.filter((toast) => toast.id !== id));
        setExitingToastIds((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        });
        dismissTimers.delete(id);
      }, TOAST_EXIT_MS),
    );
  };

  // Listen for backend AppNotification events
  onMount(async () => {
    try {
      unlisten = await listen<AppNotificationPayload>('app-notification', (event) => {
        showToast(event.payload.level, event.payload.message);
      });
    } catch (error) {
      console.error('Failed to listen for app notifications:', error);
    }
  });

  onCleanup(() => {
    unlisten?.();
    dismissTimers.forEach((timer) => clearTimeout(timer));
    dismissTimers.clear();
  });

  return (
    <ToastContext.Provider value={{ removeToast, showToast }}>
      {props.children}
      <div class="pointer-events-none fixed right-4 bottom-4 z-50 flex flex-col gap-2">
        <For each={toasts()}>
          {(toast) => (
            <Toast
              id={toast.id}
              level={toast.level}
              message={toast.message}
              exiting={exitingToastIds().has(toast.id)}
              onDismiss={removeToast}
            />
          )}
        </For>
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return context;
}
