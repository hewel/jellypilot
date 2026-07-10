import { Button, CheckboxInput } from '@jellypilot/ui';
import { listen } from '@tauri-apps/api/event';
import { For, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';

import * as styles from './DiagnosticsPanel.css';

interface BackendLogEntry {
  level: number;
  message: string;
}

interface DiagnosticEntry {
  levelName: string;
  levelTone: keyof typeof LOG_LEVEL_TONE_CLASS;
  message: string;
  time: string;
}

interface DiagnosticsPanelProps {
  compact?: boolean;
}

const MAX_DIAGNOSTICS = 200;

const LOG_LEVEL_TONE_CLASS = {
  debug: styles.badgeDebug,
  error: styles.badgeError,
  info: styles.badgeInfo,
  trace: styles.badgeTrace,
  unknown: styles.badgeDebug,
  warn: styles.badgeWarn,
} as const;

const LOG_LEVEL: Record<number, { name: string; tone: keyof typeof LOG_LEVEL_TONE_CLASS }> = {
  1: {
    name: 'TRACE',
    tone: 'trace',
  },
  2: {
    name: 'DEBUG',
    tone: 'debug',
  },
  3: {
    name: 'INFO',
    tone: 'info',
  },
  4: {
    name: 'WARN',
    tone: 'warn',
  },
  5: {
    name: 'ERROR',
    tone: 'error',
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
    name: 'UNKNOWN',
    tone: 'unknown' as const,
  };

  return {
    levelName: level.name,
    levelTone: level.tone,
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
    <div class={styles.root}>
      <div class={styles.header}>
        <p class={styles.count}>{diagnostics().length} sanitized runtime events</p>
        <Show when={!props.compact}>
          <CheckboxInput
            checked={autoScroll()}
            label="Auto-scroll"
            class={styles.checkboxRoot}
            onCheckedChange={(next) => setAutoScroll(next === true)}
          />
        </Show>
      </div>

      <div
        ref={containerRef}
        class={styles.log}
        classList={{
          [styles.compactLog]: props.compact,
          [styles.expandedLog]: !props.compact,
        }}
      >
        <Show
          when={visibleEntries().length > 0}
          fallback={
            <p class={styles.empty}>
              No diagnostics yet. Runtime events from the Rust backend will appear here.
            </p>
          }
        >
          <For each={visibleEntries()}>
            {(entry) => (
              <div class={styles.entry}>
                <div class={styles.entryInner}>
                  <span class={styles.time}>{entry.time}</span>
                  <span class={`${styles.badge} ${LOG_LEVEL_TONE_CLASS[entry.levelTone]}`}>
                    {entry.levelName}
                  </span>
                  <span class={styles.message}>{entry.message}</span>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>

      <div class={styles.actions}>
        <Show when={copyStatus() !== 'idle'}>
          <span
            role="status"
            aria-live="polite"
            class={styles.status}
            classList={{
              [styles.statusCopied]: copyStatus() === 'copied',
              [styles.statusFailed]: copyStatus() !== 'copied',
            }}
          >
            {copyStatus() === 'copied' ? 'Copied' : 'Copy failed'}
          </span>
        </Show>
        <Button
          type="button"
          onClick={copyDiagnostics}
          disabled={diagnostics().length === 0}
          variant="ghost"
          size="sm"
          class={styles.actionButton}
        >
          Copy diagnostics
        </Button>
        <Button
          type="button"
          onClick={clearDiagnostics}
          variant="ghost"
          size="sm"
          class={styles.dangerActionButton}
        >
          Clear
        </Button>
      </div>
    </div>
  );
}
