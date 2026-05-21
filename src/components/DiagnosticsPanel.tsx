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
  message: string;
  time: string;
}

interface DiagnosticsPanelProps {
  compact?: boolean;
}

const MAX_DIAGNOSTICS = 200;

const LOG_LEVEL: Record<number, { name: string; color: string }> = {
  1: { name: 'TRACE', color: 'text-outline' },
  2: { name: 'DEBUG', color: 'text-on-surface-variant' },
  3: { name: 'INFO', color: 'text-secondary' },
  4: { name: 'WARN', color: 'text-warning' },
  5: { name: 'ERROR', color: 'text-error' },
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
  };

  return {
    levelName: level.name,
    levelClass: level.color,
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
    <div class="space-y-3">
      <div class="flex items-center justify-between gap-3">
        <p class="text-body-small text-on-surface-variant">
          {diagnostics().length} sanitized runtime events
        </p>
        <Show when={!props.compact}>
          <label class="flex items-center gap-2 text-label-small text-on-surface-variant">
            <input
              type="checkbox"
              checked={autoScroll()}
              onChange={(event) => setAutoScroll(event.currentTarget.checked)}
              class="rounded border-outline-variant bg-surface-container-high text-primary focus:ring-primary/50"
            />
            Auto-scroll
          </label>
        </Show>
      </div>

      <div
        ref={containerRef}
        class={`${props.compact ? 'max-h-56' : 'max-h-96'} space-y-2 overflow-y-auto rounded-2xl border border-outline-variant/60 bg-surface-container-lowest p-2`}
      >
        <Show
          when={visibleEntries().length > 0}
          fallback={
            <p class="py-8 text-center text-body-small text-on-surface-variant">
              No diagnostics yet. Runtime events from the Rust backend will
              appear here.
            </p>
          }
        >
          <For each={visibleEntries()}>
            {(entry) => (
              <div class="diagnostic-row">
                <div class="flex flex-wrap gap-x-3 gap-y-1">
                  <span class="text-outline">{entry.time}</span>
                  <span class={entry.levelClass}>{entry.levelName}</span>
                  <span class="break-all text-on-surface-variant">
                    {entry.message}
                  </span>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>

      <div class="flex flex-wrap items-center justify-end gap-2">
        <Show when={copyStatus() !== 'idle'}>
          <span
            role="status"
            aria-live="polite"
            class={`text-label-small ${copyStatus() === 'copied' ? 'text-tertiary' : 'text-error'}`}
          >
            {copyStatus() === 'copied' ? 'Copied' : 'Copy failed'}
          </span>
        </Show>
        <button
          type="button"
          onClick={copyDiagnostics}
          disabled={diagnostics().length === 0}
          class="btn-text min-h-9 px-3 text-label-small"
        >
          Copy diagnostics
        </button>
        <button
          type="button"
          onClick={clearDiagnostics}
          class="btn-text min-h-9 px-3 text-label-small"
        >
          Clear
        </button>
      </div>
    </div>
  );
}
