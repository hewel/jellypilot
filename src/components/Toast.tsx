import { Check, Info, TriangleAlert, X } from 'lucide-solid';
import { onCleanup, onMount } from 'solid-js';

import type { NotificationLevel } from '../bindings';

export type { NotificationLevel };

interface ToastProps {
  id: string;
  level: NotificationLevel;
  message: string;
  onDismiss: (id: string) => void;
}

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

  const getStyles = () => {
    switch (props.level) {
      case 'success': {
        return 'bg-surface-container-high/90 text-on-surface border-tertiary/20 shadow-2xl backdrop-blur-md shadow-tertiary/5';
      }
      case 'error': {
        return 'bg-error-container/85 text-on-error-container border-error/25 shadow-2xl backdrop-blur-md shadow-error/10';
      }
      case 'warning': {
        return 'bg-warning-container/85 text-on-warning-container border-warning/25 shadow-2xl backdrop-blur-md shadow-warning/10';
      }
      default: {
        return 'bg-surface-container-high/90 text-on-surface border-outline-variant/60 shadow-2xl backdrop-blur-md';
      }
    }
  };

  const getIcon = () => {
    switch (props.level) {
      case 'success': {
        return <Check class="text-tertiary h-5 w-5" />;
      }
      case 'error': {
        return <X class="text-error h-5 w-5" />;
      }
      case 'warning': {
        return <TriangleAlert class="text-warning h-5 w-5" />;
      }
      default: {
        return <Info class="text-primary h-5 w-5" />;
      }
    }
  };

  return (
    <div
      class={`animate-in slide-in-from-right pointer-events-auto mb-4 flex w-full max-w-sm items-center rounded-xl border p-4 shadow-md duration-300 ${getStyles()}`}
      role="alert"
    >
      <div class="inline-flex flex-shrink-0 items-center justify-center">{getIcon()}</div>
      <div class="text-on-surface-variant ml-3 flex-1 text-[14px] leading-[20px] font-normal break-words">
        {props.message}
      </div>
      <button
        type="button"
        class="hover:bg-on-surface/10 -mx-1.5 -my-1.5 ml-auto inline-flex h-8 w-8 items-center justify-center rounded-full p-1.5 transition-colors"
        onClick={() => props.onDismiss(props.id)}
        aria-label="Close"
      >
        <span class="sr-only">Close</span>
        <X class="h-4 w-4" />
      </button>
    </div>
  );
}
