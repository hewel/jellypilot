import { createForm } from '@tanstack/solid-form';
import {
  Activity,
  Bot,
  Cast,
  ChevronDown,
  CircleCheckBig,
  ClipboardList,
  LogOut,
  Play,
  Power,
  RefreshCw,
  Settings,
  ShieldAlert,
} from 'lucide-solid';
import {
  createEffect,
  createResource,
  createSignal,
  For,
  Show,
} from 'solid-js';
import { type AppConfig, type ConnectionState, commands } from '../bindings';
import { clearSavedSession, loadSavedSession } from '../router';
import DiagnosticsPanel from './DiagnosticsPanel';
import NowPlayingCard from './NowPlayingCard';
import { useToast } from './ToastProvider';
import { PageFooter, SectionCard } from './ui';

interface OperationsConsoleProps {
  onSignedOut: () => void;
}

async function fetchConnectionState(): Promise<ConnectionState> {
  return await commands.jellyfinGetState();
}

async function fetchMpvStatus(): Promise<boolean> {
  return await commands.mpvIsConnected();
}

function statusTone(connected: boolean) {
  return connected ? 'text-secondary' : 'text-warning';
}

const COMMON_SUBTITLE_LANGUAGE_OPTIONS = [
  { code: 'eng', label: 'English' },
  { code: 'jpn', label: 'Japanese' },
  { code: 'spa', label: 'Spanish' },
  { code: 'fre', label: 'French' },
  { code: 'ger', label: 'German' },
  { code: 'ita', label: 'Italian' },
  { code: 'por', label: 'Portuguese' },
  { code: 'chi', label: 'Chinese' },
  { code: 'kor', label: 'Korean' },
] as const;

function parseSubtitleLanguageInput(value: string) {
  return value
    .split(/[\n,]+/)
    .map((language) => language.trim().toLowerCase())
    .filter((language) => language.length > 0);
}

function normalizePreferredSubtitleLanguages(
  languages: string[] | null | undefined,
) {
  const normalized: string[] = [];
  const seen = new Set<string>();

  for (const language of languages ?? []) {
    const [code] = parseSubtitleLanguageInput(language);
    if (!code || seen.has(code)) continue;
    seen.add(code);
    normalized.push(code);
  }

  return normalized;
}

function getSubtitleLanguageLabel(code: string) {
  return (
    COMMON_SUBTITLE_LANGUAGE_OPTIONS.find((option) => option.code === code)
      ?.label ?? 'Custom'
  );
}

function StatusTile(props: {
  icon: typeof CircleCheckBig;
  label: string;
  value: string;
  description: string;
  tone?: string;
}) {
  const Icon = props.icon;
  return (
    <div class="status-tile">
      <div class="mb-4 flex items-center justify-between gap-3">
        <div class={`status-tile-icon ${props.tone ?? 'text-primary'}`}>
          <Icon class="h-5 w-5" />
        </div>
        <span class="text-label-small uppercase text-on-surface-variant">
          {props.label}
        </span>
      </div>
      <p class="truncate text-title-medium text-on-surface" title={props.value}>
        {props.value}
      </p>
      <p class="mt-1 text-body-small text-on-surface-variant">
        {props.description}
      </p>
    </div>
  );
}

export default function OperationsConsole(props: OperationsConsoleProps) {
  const { showToast } = useToast();
  let introSkipperEnabledValue = true;
  let configHydrated = false;
  let introSkipperInput: HTMLInputElement | undefined;

  const [disconnecting, setDisconnecting] = createSignal(false);
  const [reconnecting, setReconnecting] = createSignal(false);
  const [signingOut, setSigningOut] = createSignal(false);
  const [confirmSignOut, setConfirmSignOut] = createSignal(false);
  const [detectingMpv, setDetectingMpv] = createSignal(false);
  const [advancedOpen, setAdvancedOpen] = createSignal(false);
  const [diagnosticsExpanded, setDiagnosticsExpanded] = createSignal(false);
  const [saveMessage, setSaveMessage] = createSignal<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);
  const [selectedSubtitleLanguages, setSelectedSubtitleLanguages] =
    createSignal<string[]>([]);
  const [subtitleLanguageInput, setSubtitleLanguageInput] = createSignal('');

  const [connectionState, { refetch: refetchConnection }] =
    createResource(fetchConnectionState);
  const [mpvConnected, { refetch: refetchMpv }] =
    createResource(fetchMpvStatus);
  const [initialConfig, { mutate: mutateConfig }] = createResource(async () => {
    try {
      return await commands.configGet();
    } catch (error) {
      console.error('Failed to load config:', error);
      return null;
    }
  });

  const form = createForm(() => ({
    defaultValues: {
      deviceName: 'JMSR',
      mpvPath: '',
      mpvArgs: '',
      keybindNext: 'Shift+n',
      keybindPrev: 'Shift+p',
      introSkipperEnabled: true,
    },
    onSubmit: async ({ value }) => {
      setSaveMessage(null);
      try {
        const cfg = initialConfig();
        const argsList = value.mpvArgs
          .split('\n')
          .map((s) => s.trim())
          .filter((s) => s.length > 0);
        const preferredSubtitleLanguages = selectedSubtitleLanguages();

        const newConfig: AppConfig = {
          deviceName: value.deviceName,
          mpvPath: value.mpvPath || null,
          mpvArgs: argsList,
          progressInterval: cfg?.progressInterval ?? 5,
          startMinimized: cfg?.startMinimized ?? false,
          introSkipperEnabled:
            introSkipperInput?.checked ?? introSkipperEnabledValue,
          preferredSubtitleLanguages,
          keybindNext: value.keybindNext,
          keybindPrev: value.keybindPrev,
        };

        const result = await commands.configSet(newConfig);
        if (result.status === 'ok') {
          setSaveMessage({
            type: 'success',
            text: 'Settings saved successfully',
          });
          setTimeout(() => setSaveMessage(null), 3000);
          refetchMpv();
          mutateConfig(newConfig);
        } else {
          setSaveMessage({ type: 'error', text: result.error.message });
        }
      } catch (error) {
        setSaveMessage({ type: 'error', text: String(error) });
      }
    },
  }));

  createEffect(() => {
    const cfg = initialConfig();
    if (cfg && !configHydrated) {
      form.setFieldValue('deviceName', cfg.deviceName ?? 'JMSR');
      form.setFieldValue('mpvPath', cfg.mpvPath ?? '');
      form.setFieldValue('mpvArgs', (cfg.mpvArgs ?? []).join('\n'));
      form.setFieldValue('keybindNext', cfg.keybindNext ?? 'Shift+n');
      form.setFieldValue('keybindPrev', cfg.keybindPrev ?? 'Shift+p');
      setSelectedSubtitleLanguages(
        normalizePreferredSubtitleLanguages(cfg.preferredSubtitleLanguages),
      );
      form.setFieldValue(
        'introSkipperEnabled',
        cfg.introSkipperEnabled ?? true,
      );
      introSkipperEnabledValue = cfg.introSkipperEnabled ?? true;
      if (introSkipperInput) {
        introSkipperInput.checked = cfg.introSkipperEnabled ?? true;
      }
      if ((cfg.mpvArgs?.length ?? 0) > 0) setAdvancedOpen(true);
      configHydrated = true;
    }
  });

  const state = () => connectionState();
  const config = () => initialConfig();

  const addPreferredSubtitleLanguages = () => {
    const additions = parseSubtitleLanguageInput(subtitleLanguageInput());
    if (additions.length === 0) return;

    setSelectedSubtitleLanguages((current) => {
      const seen = new Set(current);
      const next = [...current];

      for (const language of additions) {
        if (seen.has(language)) continue;
        seen.add(language);
        next.push(language);
      }

      return next;
    });
    setSubtitleLanguageInput('');
  };

  const removePreferredSubtitleLanguage = (language: string) => {
    setSelectedSubtitleLanguages((current) =>
      current.filter((selected) => selected !== language),
    );
  };

  const movePreferredSubtitleLanguage = (index: number, direction: -1 | 1) => {
    setSelectedSubtitleLanguages((current) => {
      const target = index + direction;
      if (target < 0 || target >= current.length) return current;

      const next = [...current];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
  };

  const handleRefresh = () => {
    refetchConnection();
    refetchMpv();
  };

  const handleReconnect = async () => {
    const session = loadSavedSession();
    if (!session) {
      showToast('error', 'No Saved Session is available. Sign in again.');
      props.onSignedOut();
      return;
    }

    setReconnecting(true);
    try {
      const result = await commands.jellyfinRestoreSession(session);
      if (result.status === 'ok') {
        showToast('success', 'Reconnected to Jellyfin');
        refetchConnection();
      } else {
        clearSavedSession();
        showToast('error', 'Could not reconnect to Jellyfin. Sign in again.');
        props.onSignedOut();
      }
    } finally {
      setReconnecting(false);
    }
  };

  const handleDisconnect = async () => {
    setDisconnecting(true);
    try {
      const result = await commands.jellyfinDisconnect();
      if (result.status === 'ok') {
        showToast('success', 'Disconnected from Jellyfin');
        refetchConnection();
      } else {
        showToast('error', result.error.message);
      }
    } finally {
      setDisconnecting(false);
    }
  };

  const handleSignOut = async () => {
    setSigningOut(true);
    try {
      const result = await commands.jellyfinClearSession();
      if (result.status === 'ok') {
        clearSavedSession();
        props.onSignedOut();
      } else {
        showToast('error', result.error.message);
      }
    } finally {
      setSigningOut(false);
      setConfirmSignOut(false);
    }
  };

  const handleDetectMpv = async () => {
    setDetectingMpv(true);
    try {
      const path = await commands.configDetectMpv();
      if (path) {
        form.setFieldValue('mpvPath', path);
        showToast('success', 'MPV detected successfully');
      } else {
        showToast(
          'warning',
          'MPV not found in PATH. Configure the path manually.',
        );
      }
    } catch (error) {
      console.error('Failed to detect MPV:', error);
      showToast('error', 'Failed to detect MPV');
    } finally {
      setDetectingMpv(false);
    }
  };

  const handleIntroSkipperToggle = (enabled: boolean) => {
    introSkipperEnabledValue = enabled;
    form.setFieldValue('introSkipperEnabled', enabled);
  };

  return (
    <div class="console-shell">
      <div class="console-container">
        <header class="hero-panel">
          <div class="relative z-10 flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between">
            <div class="max-w-2xl">
              <p class="text-label-medium uppercase text-secondary">
                JMSR Control Room
              </p>
              <h1 class="brand-type mt-2 text-display-small text-on-surface sm:text-display-medium">
                Operations Console
              </h1>
              <p class="mt-3 text-body-large text-on-surface-variant">
                Monitor the Jellyfin session, Playback Target, Player Bridge,
                and Now Playing state from one desktop surface.
              </p>
            </div>
            <div class="flex flex-wrap gap-3">
              <button
                type="button"
                onClick={handleRefresh}
                class="btn-icon"
                aria-label="Refresh status"
                title="Refresh status"
              >
                <RefreshCw class="h-5 w-5" />
              </button>
            </div>
          </div>
        </header>

        <section class="status-grid" aria-label="Operational status">
          <StatusTile
            icon={CircleCheckBig}
            label="Jellyfin"
            value={state()?.connected ? 'Connected' : 'Disconnected'}
            description={
              state()?.serverName ??
              state()?.serverUrl ??
              'Saved Session can reconnect'
            }
            tone={statusTone(state()?.connected ?? false)}
          />
          <StatusTile
            icon={Cast}
            label="Playback Target"
            value={config()?.deviceName ?? 'JMSR'}
            description="Shown in Jellyfin cast menu"
            tone="text-primary"
          />
          <StatusTile
            icon={Play}
            label="Player Bridge"
            value={mpvConnected() ? 'MPV running' : 'MPV offline'}
            description={
              mpvConnected()
                ? 'External player connected'
                : 'Start MPV or cast media'
            }
            tone={mpvConnected() ? 'text-tertiary' : 'text-warning'}
          />
          <StatusTile
            icon={Bot}
            label="Automation"
            value={
              config()?.introSkipperEnabled === false
                ? 'Manual'
                : 'Automatic Intro Skip'
            }
            description="Uses Intro Skipper ranges when available"
            tone={
              config()?.introSkipperEnabled === false
                ? 'text-on-surface-variant'
                : 'text-tertiary'
            }
          />
        </section>

        <div class="console-grid">
          <div class="space-y-6">
            <SectionCard icon={<Activity class="h-6 w-6" />} title="Connection">
              <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
                <div class="rounded-2xl bg-surface-container-high p-4">
                  <p class="text-label-small uppercase text-on-surface-variant">
                    Server
                  </p>
                  <p
                    class="truncate text-title-medium text-on-surface"
                    title={state()?.serverName ?? ''}
                  >
                    {state()?.serverName ?? 'Not connected'}
                  </p>
                </div>
                <div class="rounded-2xl bg-surface-container-high p-4 md:col-span-2">
                  <p class="text-label-small uppercase text-on-surface-variant">
                    Server URL
                  </p>
                  <p
                    class="truncate font-mono text-body-medium text-on-surface"
                    title={state()?.serverUrl ?? ''}
                  >
                    {state()?.serverUrl ??
                      'Reconnect with the Saved Session or sign in again'}
                  </p>
                </div>
                <div class="rounded-2xl bg-surface-container-high p-4">
                  <p class="text-label-small uppercase text-on-surface-variant">
                    User
                  </p>
                  <p
                    class="truncate text-title-medium text-on-surface"
                    title={state()?.userName ?? ''}
                  >
                    {state()?.userName ?? 'No active user'}
                  </p>
                </div>
              </div>
              <div class="mt-5 flex flex-wrap gap-3">
                <button
                  type="button"
                  class="btn-outlined"
                  disabled={disconnecting() || !state()?.connected}
                  onClick={handleDisconnect}
                >
                  <Power class="h-5 w-5" />
                  {disconnecting() ? 'Disconnecting...' : 'Disconnect'}
                </button>
                <Show when={!state()?.connected && loadSavedSession()}>
                  <button
                    type="button"
                    class="btn-primary"
                    disabled={reconnecting()}
                    onClick={handleReconnect}
                  >
                    {reconnecting() ? 'Reconnecting...' : 'Reconnect'}
                  </button>
                </Show>
              </div>
              <p class="mt-3 text-body-small text-on-surface-variant">
                Disconnect ends the active Jellyfin connection but keeps the
                Saved Session available for Reconnect.
              </p>
            </SectionCard>

            <NowPlayingCard
              jellyfinConnected={state()?.connected ?? false}
              onPlayerStarted={() => refetchMpv()}
            />
            <form
              onSubmit={(event) => {
                event.preventDefault();
                event.stopPropagation();
                void form.handleSubmit();
              }}
              class="space-y-6"
            >
              <SectionCard
                icon={<Settings class="h-6 w-6" />}
                title="Player Bridge settings"
              >
                <div class="space-y-5">
                  <form.Field
                    name="deviceName"
                    validators={{
                      onBlur: ({ value }) =>
                        !value.trim() ? 'Device name is required' : undefined,
                    }}
                  >
                    {(field) => (
                      <label class="block">
                        <span class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          Playback Target name
                        </span>
                        <input
                          id={field().name}
                          name={field().name}
                          type="text"
                          value={field().state.value}
                          onInput={(event) =>
                            field().handleChange(event.currentTarget.value)
                          }
                          onBlur={() => field().handleBlur()}
                          class="input-filled w-full"
                          placeholder="JMSR"
                        />
                        <Show when={field().state.meta.errors.length > 0}>
                          <p class="mt-1 text-body-small text-error">
                            {field().state.meta.errors[0]}
                          </p>
                        </Show>
                        <p class="mt-1 text-body-small text-on-surface-variant">
                          Name displayed in Jellyfin cast menu.
                        </p>
                      </label>
                    )}
                  </form.Field>

                  <form.Field name="mpvPath">
                    {(field) => (
                      <label class="block">
                        <span class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          MPV executable path
                        </span>
                        <div class="flex flex-col gap-2 sm:flex-row">
                          <input
                            id={field().name}
                            name={field().name}
                            type="text"
                            value={field().state.value}
                            onInput={(event) =>
                              field().handleChange(event.currentTarget.value)
                            }
                            onBlur={() => field().handleBlur()}
                            placeholder="Path to mpv executable"
                            class="input-filled min-w-0 flex-1"
                          />
                          <button
                            type="button"
                            onClick={handleDetectMpv}
                            disabled={detectingMpv()}
                            class="btn-secondary"
                          >
                            {detectingMpv() ? 'Detecting...' : 'Detect MPV'}
                          </button>
                        </div>
                      </label>
                    )}
                  </form.Field>

                  <button
                    type="button"
                    class="btn-text px-0"
                    aria-expanded={advancedOpen()}
                    onClick={() => setAdvancedOpen((open) => !open)}
                  >
                    <ChevronDown
                      class={`h-5 w-5 transition-transform ${advancedOpen() ? 'rotate-180' : ''}`}
                    />
                    Advanced MPV options
                  </button>

                  <Show when={advancedOpen()}>
                    <div class="space-y-5 rounded-3xl border border-outline-variant bg-surface-container-lowest p-4">
                      <form.Field name="mpvArgs">
                        {(field) => (
                          <label class="block">
                            <span class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                              Extra arguments
                            </span>
                            <textarea
                              id={field().name}
                              name={field().name}
                              value={field().state.value}
                              onInput={(event) =>
                                field().handleChange(event.currentTarget.value)
                              }
                              onBlur={() => field().handleBlur()}
                              rows={4}
                              placeholder="--fullscreen&#10;--force-window"
                              class="input-filled h-auto w-full py-3 font-mono text-body-small"
                            />
                          </label>
                        )}
                      </form.Field>

                      <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                        <form.Field
                          name="keybindNext"
                          validators={{
                            onBlur: ({ value }) =>
                              !value.trim()
                                ? 'Keybinding is required'
                                : undefined,
                          }}
                        >
                          {(field) => (
                            <label class="block">
                              <span class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                                Next episode key
                              </span>
                              <input
                                id={field().name}
                                name={field().name}
                                type="text"
                                value={field().state.value}
                                onInput={(event) =>
                                  field().handleChange(
                                    event.currentTarget.value,
                                  )
                                }
                                onBlur={() => field().handleBlur()}
                                class="input-filled w-full font-mono"
                                placeholder="Shift+n"
                              />
                            </label>
                          )}
                        </form.Field>
                        <form.Field
                          name="keybindPrev"
                          validators={{
                            onBlur: ({ value }) =>
                              !value.trim()
                                ? 'Keybinding is required'
                                : undefined,
                          }}
                        >
                          {(field) => (
                            <label class="block">
                              <span class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                                Previous episode key
                              </span>
                              <input
                                id={field().name}
                                name={field().name}
                                type="text"
                                value={field().state.value}
                                onInput={(event) =>
                                  field().handleChange(
                                    event.currentTarget.value,
                                  )
                                }
                                onBlur={() => field().handleBlur()}
                                class="input-filled w-full font-mono"
                                placeholder="Shift+p"
                              />
                            </label>
                          )}
                        </form.Field>
                      </div>
                    </div>
                  </Show>
                </div>
              </SectionCard>

              <SectionCard icon={<Bot class="h-6 w-6" />} title="Automation">
                <div class="space-y-4">
                  <div class="rounded-2xl bg-surface-container-high p-4">
                    <div class="flex flex-wrap items-start justify-between gap-3">
                      <div>
                        <h3 class="text-title-medium text-on-surface">
                          Preferred subtitle languages
                        </h3>
                        <p class="mt-1 text-body-small text-on-surface-variant">
                          Add Jellyfin language codes in fallback priority
                          order.
                        </p>
                      </div>
                      <Show when={selectedSubtitleLanguages().length > 0}>
                        <button
                          type="button"
                          class="btn-text min-w-0 px-3"
                          onClick={() => setSelectedSubtitleLanguages([])}
                        >
                          Clear all
                        </button>
                      </Show>
                    </div>

                    <div class="mt-4 flex flex-col gap-3 sm:flex-row">
                      <label class="min-w-0 flex-1">
                        <span class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          Add preferred subtitle language
                        </span>
                        <input
                          id="preferred-subtitle-language-input"
                          list="preferred-subtitle-language-options"
                          type="text"
                          value={subtitleLanguageInput()}
                          onInput={(event) =>
                            setSubtitleLanguageInput(event.currentTarget.value)
                          }
                          onKeyDown={(event) => {
                            if (event.key !== 'Enter') return;
                            event.preventDefault();
                            addPreferredSubtitleLanguages();
                          }}
                          class="input-filled w-full font-mono"
                          placeholder="eng"
                          autoComplete="off"
                        />
                        <datalist id="preferred-subtitle-language-options">
                          <For each={COMMON_SUBTITLE_LANGUAGE_OPTIONS}>
                            {(option) => (
                              <option
                                value={option.code}
                                label={`${option.code} — ${option.label}`}
                              />
                            )}
                          </For>
                        </datalist>
                      </label>
                      <button
                        type="button"
                        class="btn-secondary self-end"
                        disabled={
                          parseSubtitleLanguageInput(subtitleLanguageInput())
                            .length === 0
                        }
                        onClick={addPreferredSubtitleLanguages}
                      >
                        Add language
                      </button>
                    </div>

                    <Show
                      when={selectedSubtitleLanguages().length > 0}
                      fallback={
                        <p class="mt-4 rounded-2xl border border-dashed border-outline-variant px-4 py-3 text-body-small text-on-surface-variant">
                          No preferred subtitle languages selected. JMSR will
                          use Jellyfin and media defaults.
                        </p>
                      }
                    >
                      <ol
                        class="mt-4 space-y-2"
                        aria-label="Selected preferred subtitle languages"
                      >
                        <For each={selectedSubtitleLanguages()}>
                          {(language, index) => (
                            <li class="flex flex-wrap items-center gap-2 rounded-2xl bg-surface-container-lowest px-3 py-2">
                              <span class="flex min-w-0 flex-1 items-baseline gap-2">
                                <span class="rounded-full bg-primary-container px-3 py-1 font-mono text-label-large text-on-primary-container">
                                  {language}
                                </span>
                                <span class="text-body-small text-on-surface-variant">
                                  {getSubtitleLanguageLabel(language)}
                                </span>
                              </span>
                              <button
                                type="button"
                                class="btn-text min-w-0 px-2"
                                disabled={index() === 0}
                                aria-label={`Move ${language} up`}
                                onClick={() =>
                                  movePreferredSubtitleLanguage(index(), -1)
                                }
                              >
                                Move up
                              </button>
                              <button
                                type="button"
                                class="btn-text min-w-0 px-2"
                                disabled={
                                  index() ===
                                  selectedSubtitleLanguages().length - 1
                                }
                                aria-label={`Move ${language} down`}
                                onClick={() =>
                                  movePreferredSubtitleLanguage(index(), 1)
                                }
                              >
                                Move down
                              </button>
                              <button
                                type="button"
                                class="btn-text min-w-0 px-2"
                                aria-label={`Remove ${language}`}
                                onClick={() =>
                                  removePreferredSubtitleLanguage(language)
                                }
                              >
                                Remove
                              </button>
                            </li>
                          )}
                        </For>
                      </ol>
                    </Show>
                  </div>
                  <label
                    for="intro-skipper-enabled"
                    class="flex cursor-pointer items-center justify-between gap-4 rounded-2xl bg-surface-container-high px-4 py-3"
                  >
                    <span>
                      <span class="block text-title-medium text-on-surface">
                        Automatic Intro Skip
                      </span>
                      <span class="text-body-small text-on-surface-variant">
                        Use Intro Skipper ranges when available.
                      </span>
                    </span>
                    <input
                      id="intro-skipper-enabled"
                      name="introSkipperEnabled"
                      type="checkbox"
                      aria-label="Automatic Intro Skip"
                      ref={(el) => {
                        introSkipperInput = el;
                      }}
                      checked={introSkipperEnabledValue}
                      onInput={(event) =>
                        handleIntroSkipperToggle(event.currentTarget.checked)
                      }
                      onChange={(event) =>
                        handleIntroSkipperToggle(event.currentTarget.checked)
                      }
                      class="h-6 w-6 rounded border-outline text-primary focus:ring-primary"
                    />
                  </label>
                </div>
              </SectionCard>

              <form.Subscribe selector={(formState) => formState.isSubmitting}>
                {(isSubmitting) => (
                  <button
                    type="submit"
                    disabled={isSubmitting()}
                    class="btn-primary w-full"
                  >
                    {isSubmitting() ? 'Saving...' : 'Save Settings'}
                  </button>
                )}
              </form.Subscribe>

              <Show when={saveMessage()}>
                <div
                  class={`rounded-2xl p-4 text-center text-body-medium font-medium ${saveMessage()?.type === 'success' ? 'bg-tertiary-container text-on-tertiary-container' : 'bg-error-container text-on-error-container'}`}
                >
                  {saveMessage()?.text}
                </div>
              </Show>
            </form>
          </div>

          <aside class="space-y-6">
            <SectionCard
              icon={<ClipboardList class="h-6 w-6" />}
              title="Diagnostics"
              trailing={
                <button
                  type="button"
                  class="btn-text min-w-0 px-3"
                  onClick={() =>
                    setDiagnosticsExpanded((expanded) => !expanded)
                  }
                  aria-expanded={diagnosticsExpanded()}
                  aria-label="Toggle diagnostics"
                >
                  {diagnosticsExpanded() ? 'Collapse' : 'Expand'}
                </button>
              }
            >
              <DiagnosticsPanel compact={!diagnosticsExpanded()} />
            </SectionCard>

            <section class="card-filled border-error/30">
              <div class="flex items-start gap-3">
                <ShieldAlert class="mt-1 h-5 w-5 text-error" />
                <div>
                  <h2 class="text-title-medium text-on-surface">Session</h2>
                  <p class="mt-1 text-body-small text-on-surface-variant">
                    Sign out removes the Saved Session and requires
                    authentication before Reconnect is available.
                  </p>
                </div>
              </div>
              <button
                type="button"
                class="btn-outlined mt-5 w-full border-error/60 text-error hover:bg-error/10"
                onClick={() => setConfirmSignOut(true)}
              >
                <LogOut class="h-5 w-5" />
                Sign out
              </button>
            </section>

            <PageFooter />
          </aside>
        </div>
      </div>

      <Show when={confirmSignOut()}>
        <div
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
          role="presentation"
        >
          <div
            class="card-elevated max-w-md"
            role="dialog"
            aria-modal="true"
            aria-labelledby="sign-out-title"
          >
            <h2 id="sign-out-title" class="text-title-large text-on-surface">
              Sign out?
            </h2>
            <p class="mt-3 text-body-medium text-on-surface-variant">
              This removes the Saved Session and you’ll need to authenticate
              again before reconnecting.
            </p>
            <div class="mt-6 flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
              <button
                type="button"
                class="btn-secondary"
                onClick={() => setConfirmSignOut(false)}
                disabled={signingOut()}
              >
                Cancel
              </button>
              <button
                type="button"
                class="btn-outlined border-error/60 text-error hover:bg-error/10"
                onClick={handleSignOut}
                disabled={signingOut()}
              >
                {signingOut() ? 'Signing out...' : 'Sign out'}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}
