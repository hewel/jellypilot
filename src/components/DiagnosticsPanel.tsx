import { Checkbox } from '@ark-ui/solid/checkbox';
import { listen } from '@tauri-apps/api/event';
import { For, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';

import { Button } from './ui';

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

const LOG_LEVEL: Record<number, { name: string; color: string; badge: string }> = {
  1: {
    badge: 'bg-surface-container-highest border-outline-variant/40 text-outline',
    color: 'text-outline',
    name: 'TRACE',
  },
  2: {
    badge: 'bg-surface-container-highest border-outline/30 text-on-surface-variant',
    color: 'text-on-surface-variant',
    name: 'DEBUG',
  },
  3: {
    badge:
      'bg-secondary-container/30 border-secondary/30 text-secondary shadow-[0_0_6px_rgba(129,140,248,0.1)]',
    color: 'text-secondary',
    name: 'INFO',
  },
  4: {
    badge:
      'bg-warning-container/30 border-warning/30 text-warning shadow-[0_0_6px_rgba(246,199,104,0.1)]',
    color: 'text-warning',
    name: 'WARN',
  },
  5: {
    badge:
      'bg-error-container/30 border-error/30 text-error shadow-[0_0_6px_rgba(255,107,122,0.1)]',
    color: 'text-error',
    name: 'ERROR',
  },
};

const SENSITIVE_QUERY_PARAM =
  /([?&](?:api_key|access_token|token|password|auth|authorization)=)[^&\s]+/gi;
const BEARER_TOKEN = /(bearer\s+)[^\s]+/gi;

function formatDiagnosticTime(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    hour12: false,
    minute: '2-digit',
    second: '2-digit',
  }).format(date);
}

function sanitizeDiagnosticMessage(message: string) {
  return message
    .replace(SENSITIVE_QUERY_PARAM, '$1[REDACTED]')
    .replace(BEARER_TOKEN, '$1[REDACTED]');
}

function toDiagnosticEntry(entry: BackendLogEntry): DiagnosticEntry {
  const level = LOG_LEVEL[entry.level] ?? {
    badge: 'bg-surface-container-highest text-on-surface-variant',
    color: 'text-on-surface-variant',
    name: 'UNKNOWN',
  };

  return {
    badgeClass: level.badge,
    levelClass: level.color,
    levelName: level.name,
    message: sanitizeDiagnosticMessage(entry.message),
    time: formatDiagnosticTime(new Date()),
  };
}

function formatDiagnosticsForClipboard(entries: DiagnosticEntry[]) {
  return entries.map((entry) => `[${entry.time}] ${entry.levelName} ${entry.message}`).join('\n');
}

export default function DiagnosticsPanel(props: DiagnosticsPanelProps) {
  const [diagnostics, setDiagnostics] = createSignal<DiagnosticEntry[]>([]);
  const [autoScroll, setAutoScroll] = createSignal(true);
  const [copyStatus, setCopyStatus] = createSignal<'idle' | 'copied' | 'failed'>('idle');
  let containerRef: HTMLDivElement | undefined;

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;

    listen<BackendLogEntry>('log://log', (event) => {
      setDiagnostics((prev) => [...prev, toDiagnosticEntry(event.payload)].slice(-MAX_DIAGNOSTICS));
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

  const visibleEntries = () => (props.compact ? diagnostics().slice(-5) : diagnostics());

  const clearDiagnostics = () => {
    setDiagnostics([]);
    setCopyStatus('idle');
  };

  const copyDiagnostics = async () => {
    try {
      await navigator.clipboard.writeText(formatDiagnosticsForClipboard(diagnostics()));
      setCopyStatus('copied');
    } catch {
      setCopyStatus('failed');
    }
  };

  return (
    <div class="space-y-4">
      <div class="flex items-center justify-between gap-3 px-1">
        <p class="text-on-surface-variant/80 font-mono text-[11px] font-semibold tabular-nums">
          {diagnostics().length} sanitized runtime events
        </p>
        <Show when={!props.compact}>
          <Checkbox.Root
            checked={autoScroll()}
            onCheckedChange={(details) => setAutoScroll(details.checked === true)}
            class="text-on-surface-variant/95 inline-flex cursor-pointer items-center gap-2.5 align-top text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase transition-opacity select-none disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Checkbox.Control class="border-outline bg-surface-container-high text-on-primary hover:border-primary/60 data-[state=checked]:border-primary data-[state=checked]:from-primary data-[state=checked]:to-primary-gradient-end data-[state=indeterminate]:border-primary data-[state=indeterminate]:from-primary data-[state=indeterminate]:to-primary-gradient-end data-[focus-visible]:ring-primary/50 data-[focus-visible]:ring-offset-background inline-flex h-5.5 w-5.5 shrink-0 items-center justify-center rounded-lg border text-[11px] leading-none transition-[background-color,border-color,box-shadow] duration-200 data-[focus-visible]:ring-2 data-[focus-visible]:ring-offset-2 data-[focus-visible]:outline-none data-[state=checked]:bg-gradient-to-br data-[state=indeterminate]:bg-gradient-to-br">
              <Checkbox.Indicator class="flex items-center justify-center font-black">
                ✓
              </Checkbox.Indicator>
            </Checkbox.Control>
            <Checkbox.Label class="cursor-pointer select-none">Auto-scroll</Checkbox.Label>
            <Checkbox.HiddenInput />
          </Checkbox.Root>
        </Show>
      </div>

      <div
        ref={containerRef}
        class={`${props.compact ? 'max-h-56' : 'max-h-96'} border-outline-variant bg-surface-container-lowest/60 space-y-2.5 overflow-y-auto rounded-2xl border p-3 shadow-inner backdrop-blur-sm`}
      >
        <Show
          when={visibleEntries().length > 0}
          fallback={
            <p class="text-on-surface-variant/60 py-10 text-center font-mono text-[12px] leading-[16px]">
              No diagnostics yet. Runtime events from the Rust backend will appear here.
            </p>
          }
        >
          <For each={visibleEntries()}>
            {(entry) => (
              <div class="border-outline-variant/40 bg-surface-container-lowest/70 text-on-surface-variant hover:bg-surface-container-lowest/90 hover:border-outline-variant/60 relative overflow-hidden rounded-xl border px-3.5 py-2 font-mono text-[12px] leading-5 transition-colors">
                <div class="relative z-10 flex flex-wrap items-center gap-x-3 gap-y-1.5">
                  <span class="text-outline font-semibold select-none">{entry.time}</span>
                  <span
                    class={`rounded border px-2 py-0.5 text-[10px] font-bold tracking-wider select-none ${entry.badgeClass}`}
                  >
                    {entry.levelName}
                  </span>
                  <span class="text-on-surface-variant font-medium break-all">{entry.message}</span>
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
            class={`text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase ${copyStatus() === 'copied' ? 'text-tertiary drop-shadow-[0_0_6px_rgba(79,227,177,0.2)]' : 'text-error'}`}
          >
            {copyStatus() === 'copied' ? 'Copied' : 'Copy failed'}
          </span>
        </Show>
        <Button
          type="button"
          onClick={copyDiagnostics}
          disabled={diagnostics().length === 0}
          variant="text"
          size="sm"
          class="border-outline-variant hover:border-secondary hover:bg-secondary/5 text-on-surface-variant/90 rounded-xl border text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase"
        >
          Copy diagnostics
        </Button>
        <Button
          type="button"
          onClick={clearDiagnostics}
          variant="text"
          size="sm"
          class="border-outline-variant hover:border-error hover:bg-error/5 text-on-surface-variant/90 hover:text-error rounded-xl border text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase"
        >
          Clear
        </Button>
      </div>
    </div>
  );
}
