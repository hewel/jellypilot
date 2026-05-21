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

const MAX_DIAGNOSTICS = 200;

const LOG_LEVEL: Record<number, { name: string; color: string }> = {
  1: { name: 'TRACE', color: 'text-outline' },
  2: { name: 'DEBUG', color: 'text-on-surface-variant' },
  3: { name: 'INFO', color: 'text-primary' },
  4: { name: 'WARN', color: 'text-secondary' },
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
    color: 'text-gray-400',
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

export default function DiagnosticsPanel() {
  const [diagnostics, setDiagnostics] = createSignal<DiagnosticEntry[]>([]);
  const [expanded, setExpanded] = createSignal(false);
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
    if (autoScroll() && containerRef) {
      diagnostics();
      containerRef.scrollTop = containerRef.scrollHeight;
    }
  });

  const toggleExpand = () => {
    setExpanded(!expanded());
  };

  const handleScroll = () => {
    if (containerRef) {
      const { scrollTop, scrollHeight, clientHeight } = containerRef;
      setAutoScroll(scrollTop + clientHeight >= scrollHeight - 10);
    }
  };

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
    <div class="card-outlined overflow-hidden p-0">
      <button
        type="button"
        class="w-full flex items-center justify-between p-4 hover:bg-surface-container-high/50 transition-colors group"
        onClick={toggleExpand}
        aria-expanded={expanded()}
      >
        <div class="flex items-center gap-3">
          <div class="p-2 bg-primary/10 rounded-full group-hover:bg-primary/20 transition-colors">
            <svg
              class="w-5 h-5 text-primary"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svg>
          </div>
          <div>
            <h2 class="text-title-medium text-on-surface text-left">
              Diagnostics
            </h2>
            <div class="text-on-surface-variant text-label-small mt-0.5 flex gap-2">
              <span>Recent runtime events for support troubleshooting</span>
              <span class="px-1.5 py-0.5 bg-surface-container-highest rounded-md font-mono">
                {diagnostics().length} entries
              </span>
            </div>
          </div>
        </div>
        <svg
          class={`w-5 h-5 text-on-surface-variant transition-transform duration-300 ${expanded() ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      <Show when={expanded()}>
        <div class="border-t border-outline-variant/30 animate-in slide-in-from-top-2 fade-in duration-200">
          <div class="flex items-center justify-between px-4 py-2 bg-surface-container-low">
            <label class="flex items-center gap-1.5 text-label-small text-on-surface-variant cursor-pointer">
              <input
                type="checkbox"
                checked={autoScroll()}
                onChange={(e) => setAutoScroll(e.target.checked)}
                class="rounded bg-surface-container-high border-outline-variant text-primary focus:ring-primary/50"
              />
              Auto-scroll
            </label>

            <div class="flex items-center gap-2">
              <Show when={copyStatus() !== 'idle'}>
                <span
                  role="status"
                  aria-live="polite"
                  class={`text-label-small ${copyStatus() === 'copied' ? 'text-primary' : 'text-error'}`}
                >
                  {copyStatus() === 'copied' ? 'Copied' : 'Copy failed'}
                </span>
              </Show>
              <button
                type="button"
                onClick={copyDiagnostics}
                disabled={diagnostics().length === 0}
                class="btn-text h-8 min-w-0 px-3 text-label-small disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Copy diagnostics
              </button>
              <button
                type="button"
                onClick={clearDiagnostics}
                class="btn-text h-8 min-w-0 px-3 text-label-small"
              >
                Clear
              </button>
            </div>
          </div>

          <div
            ref={containerRef}
            onScroll={handleScroll}
            class="h-64 overflow-y-auto bg-surface-container-lowest/50 font-mono text-body-small p-2 space-y-0.5"
          >
            <Show
              when={diagnostics().length > 0}
              fallback={
                <p class="text-outline text-center py-8">
                  No diagnostics yet. Runtime events from the Rust backend will
                  appear here.
                </p>
              }
            >
              <For each={diagnostics()}>
                {(entry) => (
                  <div class="flex gap-2 hover:bg-on-surface/5 px-1 rounded">
                    <span class="shrink-0 w-20 text-outline">{entry.time}</span>
                    <span class={`shrink-0 w-12 ${entry.levelClass}`}>
                      {entry.levelName}
                    </span>
                    <span class="text-on-surface-variant break-all">
                      {entry.message}
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}
