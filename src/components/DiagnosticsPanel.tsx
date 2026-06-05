import { Checkbox } from '@ark-ui/solid/checkbox';
import { listen } from '@tauri-apps/api/event';
import {
  createEffect,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
} from 'solid-js';

interface BackendLogEntry {
  level: number;
  message: string;
}

interface DiagnosticEntry {
  levelName: string;
  levelClass: string;
  badgeClass: string;
  message: string;
  time: string;
}

interface DiagnosticsPanelProps {
  compact?: boolean;
}

const MAX_DIAGNOSTICS = 200;

const LOG_LEVEL: Record<
  number,
  { name: string; color: string; badge: string }
> = {
  1: {
    name: 'TRACE',
    color: 'text-outline',
    badge:
      'bg-surface-container-highest border-outline-variant/40 text-outline',
  },
  2: {
    name: 'DEBUG',
    color: 'text-on-surface-variant',
    badge:
      'bg-surface-container-highest border-outline/30 text-on-surface-variant',
  },
  3: {
    name: 'INFO',
    color: 'text-secondary',
    badge:
      'bg-secondary-container/30 border-secondary/30 text-secondary shadow-[0_0_6px_rgba(57,213,255,0.1)]',
  },
  4: {
    name: 'WARN',
    color: 'text-warning',
    badge:
      'bg-warning-container/30 border-warning/30 text-warning shadow-[0_0_6px_rgba(246,199,104,0.1)]',
  },
  5: {
    name: 'ERROR',
    color: 'text-error',
    badge:
      'bg-error-container/30 border-error/30 text-error shadow-[0_0_6px_rgba(255,107,122,0.1)]',
  },
};

const SENSITIVE_QUERY_PARAM =
  /([?&](?:api_key|access_token|token|password|auth|authorization)=)[^&\s]+/gi;
const BEARER_TOKEN = /(bearer\s+)[^\s]+/gi;

function formatDiagnosticTime(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(date);
}

function sanitizeDiagnosticMessage(message: string) {
  return message
    .replace(SENSITIVE_QUERY_PARAM, '$1[REDACTED]')
    .replace(BEARER_TOKEN, '$1[REDACTED]');
}

function toDiagnosticEntry(entry: BackendLogEntry): DiagnosticEntry {
  const level = LOG_LEVEL[entry.level] ?? {
    name: 'UNKNOWN',
    color: 'text-on-surface-variant',
    badge: 'bg-surface-container-highest text-on-surface-variant',
  };

  return {
    levelName: level.name,
    levelClass: level.color,
    badgeClass: level.badge,
    message: sanitizeDiagnosticMessage(entry.message),
    time: formatDiagnosticTime(new Date()),
  };
}

function formatDiagnosticsForClipboard(entries: DiagnosticEntry[]) {
  return entries
    .map((entry) => `[${entry.time}] ${entry.levelName} ${entry.message}`)
    .join('\n');
}

export default function DiagnosticsPanel(props: DiagnosticsPanelProps) {
  const [diagnostics, setDiagnostics] = createSignal<DiagnosticEntry[]>([]);
  const [autoScroll, setAutoScroll] = createSignal(true);
  const [copyStatus, setCopyStatus] = createSignal<
    'idle' | 'copied' | 'failed'
  >('idle');
  let containerRef: HTMLDivElement | undefined;

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;

    listen<BackendLogEntry>('log://log', (event) => {
      setDiagnostics((prev) =>
        [...prev, toDiagnosticEntry(event.payload)].slice(-MAX_DIAGNOSTICS),
      );
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
        return;
      }
      cleanup = unlisten;
    });

    onCleanup(() => {
      disposed = true;
      cleanup?.();
    });
  });

  createEffect(() => {
    if (!props.compact && autoScroll() && containerRef) {
      diagnostics();
      containerRef.scrollTop = containerRef.scrollHeight;
    }
  });

  const visibleEntries = () =>
    props.compact ? diagnostics().slice(-5) : diagnostics();

  const clearDiagnostics = () => {
    setDiagnostics([]);
    setCopyStatus('idle');
  };

  const copyDiagnostics = async () => {
    try {
      await navigator.clipboard.writeText(
        formatDiagnosticsForClipboard(diagnostics()),
      );
      setCopyStatus('copied');
    } catch {
      setCopyStatus('failed');
    }
  };

  return (
    <div class="space-y-4">
      <div class="flex items-center justify-between gap-3 px-1">
        <p class="font-mono text-[11px] font-semibold text-on-surface-variant/80">
          {diagnostics().length} sanitized runtime events
        </p>
        <Show when={!props.compact}>
          <Checkbox.Root
            checked={autoScroll()}
            onCheckedChange={(details) =>
              setAutoScroll(details.checked === true)
            }
            class="ark-checkbox text-label-small text-on-surface-variant/95"
          >
            <Checkbox.Control class="ark-checkbox__control">
              <Checkbox.Indicator class="ark-checkbox__indicator">
                ✓
              </Checkbox.Indicator>
            </Checkbox.Control>
            <Checkbox.Label class="cursor-pointer select-none">
              Auto-scroll
            </Checkbox.Label>
            <Checkbox.HiddenInput />
          </Checkbox.Root>
        </Show>
      </div>

      <div
        ref={containerRef}
        class={`${props.compact ? 'max-h-56' : 'max-h-96'} space-y-2.5 overflow-y-auto rounded-2xl border border-outline-variant bg-surface-container-lowest/60 p-3 shadow-inner backdrop-blur-sm`}
      >
        <Show
          when={visibleEntries().length > 0}
          fallback={
            <p class="py-10 text-center font-mono text-body-small text-on-surface-variant/60">
              No diagnostics yet. Runtime events from the Rust backend will
              appear here.
            </p>
          }
        >
          <For each={visibleEntries()}>
            {(entry) => (
              <div class="diagnostic-row relative overflow-hidden">
                <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5 relative z-10">
                  <span class="text-outline font-semibold select-none">
                    {entry.time}
                  </span>
                  <span
                    class={`px-2 py-0.5 rounded text-[10px] font-bold border tracking-wider select-none ${entry.badgeClass}`}
                  >
                    {entry.levelName}
                  </span>
                  <span class="break-all text-on-surface-variant font-medium">
                    {entry.message}
                  </span>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>

      <div class="flex flex-wrap items-center justify-end gap-3 px-1">
        <Show when={copyStatus() !== 'idle'}>
          <span
            role="status"
            aria-live="polite"
            class={`text-label-small font-bold ${copyStatus() === 'copied' ? 'text-tertiary drop-shadow-[0_0_6px_rgba(79,227,177,0.2)]' : 'text-error'}`}
          >
            {copyStatus() === 'copied' ? 'Copied' : 'Copy failed'}
          </span>
        </Show>
        <button
          type="button"
          onClick={copyDiagnostics}
          disabled={diagnostics().length === 0}
          class="btn-text min-h-9 px-3.5 text-label-small border border-outline-variant hover:border-secondary hover:bg-secondary/5 rounded-xl font-bold"
        >
          Copy diagnostics
        </button>
        <button
          type="button"
          onClick={clearDiagnostics}
          class="btn-text min-h-9 px-3.5 text-label-small border border-outline-variant hover:border-error hover:bg-error/5 rounded-xl font-bold hover:text-error"
        >
          Clear
        </button>
      </div>
    </div>
  );
}
