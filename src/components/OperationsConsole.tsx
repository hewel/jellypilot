import { Checkbox } from '@ark-ui/solid/checkbox';
import { Collapsible } from '@ark-ui/solid/collapsible';
import { Combobox, createListCollection } from '@ark-ui/solid/combobox';
import { Dialog } from '@ark-ui/solid/dialog';
import { Field as ArkField } from '@ark-ui/solid/field';
import { TagsInput } from '@ark-ui/solid/tags-input';
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
  createMemo,
  createResource,
  createSignal,
  For,
  Show,
} from 'solid-js';
import { Portal } from 'solid-js/web';
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
  let configHydrated = false;
  type PendingSave = {
    config: AppConfig;
    onSuccess?: () => void;
    onError?: (message: string) => void;
  };
  let lastSavedConfig: AppConfig | null = null;
  let saveInFlight = false;
  let pendingSave: PendingSave | null = null;
  let latestConfigSnapshot: AppConfig | null = null;
  let clearPlayerBridgeStatusTimer: ReturnType<typeof setTimeout> | null = null;

  const [disconnecting, setDisconnecting] = createSignal(false);
  const [reconnecting, setReconnecting] = createSignal(false);
  const [signingOut, setSigningOut] = createSignal(false);
  const [confirmSignOut, setConfirmSignOut] = createSignal(false);
  const [detectingMpv, setDetectingMpv] = createSignal(false);
  const [advancedOpen, setAdvancedOpen] = createSignal(false);
  const [diagnosticsExpanded, setDiagnosticsExpanded] = createSignal(false);
  const [playerBridgeSaveStatus, setPlayerBridgeSaveStatus] = createSignal<{
    type: 'saving' | 'saved' | 'error';
    text: string;
  } | null>(null);
  const [introSkipperDraft, setIntroSkipperDraft] = createSignal<
    boolean | null
  >(null);
  const [introSkipperSaving, setIntroSkipperSaving] = createSignal(false);
  const [introSkipperError, setIntroSkipperError] = createSignal<string | null>(
    null,
  );
  const [selectedSubtitleLanguages, setSelectedSubtitleLanguages] =
    createSignal<string[]>([]);
  const [subtitleLanguageInput, setSubtitleLanguageInput] = createSignal('');
  const filteredSubtitleLanguageOptions = createMemo(() => {
    const query = subtitleLanguageInput().trim().toLowerCase();
    if (!query) return COMMON_SUBTITLE_LANGUAGE_OPTIONS;
    return COMMON_SUBTITLE_LANGUAGE_OPTIONS.filter(
      (option) =>
        option.code.includes(query) ||
        option.label.toLowerCase().includes(query),
    );
  });
  const subtitleLanguageCollection = createMemo(() =>
    createListCollection({
      items: filteredSubtitleLanguageOptions().map((option) => ({
        value: option.code,
        label: `${option.code} — ${option.label}`,
      })),
    }),
  );

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
  }));

  createEffect(() => {
    const cfg = initialConfig();
    if (cfg && !configHydrated) {
      lastSavedConfig = cfg;
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
      if ((cfg.mpvArgs?.length ?? 0) > 0) setAdvancedOpen(true);
      configHydrated = true;
    }
  });

  const state = () => connectionState();
  const config = () => initialConfig();
  const introSkipperEnabled = () =>
    introSkipperDraft() ?? config()?.introSkipperEnabled ?? true;
  const showPlayerBridgeStatus = (
    type: 'saving' | 'saved' | 'error',
    text: string,
  ) => {
    if (clearPlayerBridgeStatusTimer) {
      clearTimeout(clearPlayerBridgeStatusTimer);
      clearPlayerBridgeStatusTimer = null;
    }
    setPlayerBridgeSaveStatus({ type, text });
    if (type === 'saved') {
      clearPlayerBridgeStatusTimer = setTimeout(
        () => setPlayerBridgeSaveStatus(null),
        3000,
      );
    }
  };

  const parseMpvArgs = (value: string) =>
    value
      .split('\n')
      .map((arg) => arg.trim())
      .filter((arg) => arg.length > 0);

  const buildConfigSnapshot = (overrides: Partial<AppConfig>) => {
    const saved =
      pendingSave?.config ??
      latestConfigSnapshot ??
      lastSavedConfig ??
      config();
    if (!saved) return null;

    return {
      ...saved,
      ...overrides,
    };
  };

  const processConfigSaveQueue = async () => {
    if (saveInFlight) return;
    saveInFlight = true;

    try {
      while (pendingSave) {
        const nextSave = pendingSave;
        pendingSave = null;
        showPlayerBridgeStatus('saving', 'Saving…');

        const result = await commands.configSet(nextSave.config);
        if (result.status === 'ok') {
          lastSavedConfig = nextSave.config;
          mutateConfig(nextSave.config);
          refetchMpv();
          nextSave.onSuccess?.();
          showPlayerBridgeStatus('saved', 'Saved');
        } else {
          nextSave.onError?.(result.error.message);
          showPlayerBridgeStatus('error', result.error.message);
          showToast('error', result.error.message);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      showPlayerBridgeStatus('error', message);
      showToast('error', message);
    } finally {
      saveInFlight = false;
      if (pendingSave) void processConfigSaveQueue();
    }
  };

  const queueConfigSave = (
    snapshot: AppConfig | null,
    callbacks: Omit<PendingSave, 'config'> = {},
  ) => {
    if (!snapshot) return;
    latestConfigSnapshot = snapshot;
    pendingSave = { config: snapshot, ...callbacks };
    void processConfigSaveQueue();
  };

  const saveTextSetting = (
    field: 'deviceName' | 'mpvPath' | 'mpvArgs' | 'keybindNext' | 'keybindPrev',
    value: string,
  ) => {
    const saved = lastSavedConfig ?? config();
    const desired = latestConfigSnapshot ?? saved;
    if (!saved || !desired) return;

    if (field === 'deviceName' && value.trim().length === 0) return;
    if (field === 'keybindNext' && value.trim().length === 0) return;
    if (field === 'keybindPrev' && value.trim().length === 0) return;

    const override =
      field === 'mpvArgs'
        ? { mpvArgs: parseMpvArgs(value) }
        : field === 'mpvPath'
          ? { mpvPath: value.trim().length > 0 ? value : null }
          : { [field]: value };

    if (field === 'mpvArgs') {
      const nextArgs = override.mpvArgs ?? [];
      if (
        nextArgs.length === (desired.mpvArgs?.length ?? 0) &&
        nextArgs.every((arg, index) => arg === desired.mpvArgs?.[index])
      )
        return;
    } else if (field === 'mpvPath') {
      if (override.mpvPath === desired.mpvPath) return;
    } else if (override[field] === desired[field]) {
      return;
    }

    queueConfigSave(buildConfigSnapshot(override));
  };

  const savePreferredSubtitleLanguages = (languages: string[]) => {
    const desired = latestConfigSnapshot ?? lastSavedConfig ?? config();
    if (
      desired &&
      languages.length === (desired.preferredSubtitleLanguages?.length ?? 0) &&
      languages.every(
        (language, index) =>
          language === desired.preferredSubtitleLanguages?.[index],
      )
    )
      return;

    queueConfigSave(
      buildConfigSnapshot({ preferredSubtitleLanguages: languages }),
    );
  };

  const saveIntroSkipperSetting = (enabled: boolean) => {
    const previous = introSkipperEnabled();
    const desired = latestConfigSnapshot ?? lastSavedConfig ?? config();
    if (desired?.introSkipperEnabled === enabled) return;

    setIntroSkipperDraft(enabled);
    setIntroSkipperSaving(true);
    setIntroSkipperError(null);
    queueConfigSave(buildConfigSnapshot({ introSkipperEnabled: enabled }), {
      onSuccess: () => {
        setIntroSkipperDraft(null);
        setIntroSkipperSaving(false);
      },
      onError: (message) => {
        setIntroSkipperDraft(previous);
        setIntroSkipperSaving(false);
        setIntroSkipperError(message);
        form.setFieldValue('introSkipperEnabled', previous);
      },
    });
  };

  const addPreferredSubtitleLanguageCodes = (languages: string[]) => {
    if (languages.length === 0) return;

    let nextLanguages: string[] = [];
    setSelectedSubtitleLanguages((current) => {
      const seen = new Set(current);
      const next = [...current];

      for (const language of languages) {
        const [code] = parseSubtitleLanguageInput(language);
        if (!code || seen.has(code)) continue;
        seen.add(code);
        next.push(code);
      }

      nextLanguages = next;
      return next;
    });
    savePreferredSubtitleLanguages(nextLanguages);
    setSubtitleLanguageInput('');
  };

  const addPreferredSubtitleLanguages = () => {
    addPreferredSubtitleLanguageCodes(
      parseSubtitleLanguageInput(subtitleLanguageInput()),
    );
  };

  const removePreferredSubtitleLanguage = (language: string) => {
    let nextLanguages: string[] = [];
    setSelectedSubtitleLanguages((current) => {
      nextLanguages = current.filter((selected) => selected !== language);
      return nextLanguages;
    });
    savePreferredSubtitleLanguages(nextLanguages);
  };

  const clearPreferredSubtitleLanguages = () => {
    setSelectedSubtitleLanguages([]);
    savePreferredSubtitleLanguages([]);
  };

  const movePreferredSubtitleLanguage = (index: number, direction: -1 | 1) => {
    let nextLanguages: string[] | null = null;
    setSelectedSubtitleLanguages((current) => {
      const target = index + direction;
      if (target < 0 || target >= current.length) return current;

      const next = [...current];
      [next[index], next[target]] = [next[target], next[index]];
      nextLanguages = next;
      return next;
    });
    if (nextLanguages) savePreferredSubtitleLanguages(nextLanguages);
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
        queueConfigSave(buildConfigSnapshot({ mpvPath: path }));
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
    form.setFieldValue('introSkipperEnabled', enabled);
    saveIntroSkipperSetting(enabled);
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
          <button
            type="button"
            class="status-tile w-full text-left transition hover:border-primary/50 hover:bg-surface-container-high focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
            aria-pressed={introSkipperEnabled()}
            onClick={() => handleIntroSkipperToggle(!introSkipperEnabled())}
          >
            <div class="mb-4 flex items-center justify-between gap-3">
              <div
                class={`status-tile-icon ${introSkipperEnabled() ? 'text-tertiary' : 'text-on-surface-variant'}`}
              >
                <Bot class="h-5 w-5" />
              </div>
              <span class="text-label-small uppercase text-on-surface-variant">
                Automatic Intro Skip
              </span>
            </div>
            <p class="truncate text-title-medium text-on-surface">
              {introSkipperEnabled() ? 'Enabled' : 'Manual'}
            </p>
            <p class="mt-1 text-body-small text-on-surface-variant">
              {introSkipperSaving()
                ? 'Saving preference…'
                : 'Uses Intro Skipper ranges when available'}
            </p>
          </button>
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
            <form class="space-y-6">
              <SectionCard
                icon={<Settings class="h-6 w-6" />}
                title="Player Bridge settings"
                trailing={
                  <Show when={playerBridgeSaveStatus()}>
                    {(status) => (
                      <span
                        class={`text-label-small font-semibold ${status().type === 'error' ? 'text-error' : 'text-secondary'}`}
                      >
                        {status().text}
                      </span>
                    )}
                  </Show>
                }
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
                      <ArkField.Root
                        class="block"
                        invalid={field().state.meta.errors.length > 0}
                      >
                        <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          Playback Target name
                        </ArkField.Label>
                        <ArkField.Input
                          id={field().name}
                          name={field().name}
                          type="text"
                          value={field().state.value}
                          onInput={(event) =>
                            field().handleChange(event.currentTarget.value)
                          }
                          onBlur={(event) => {
                            field().handleBlur();
                            saveTextSetting(
                              'deviceName',
                              event.currentTarget.value,
                            );
                          }}
                          class="input-filled w-full"
                          placeholder="JMSR"
                        />
                        <Show when={field().state.meta.errors.length > 0}>
                          <ArkField.ErrorText class="mt-1 text-body-small text-error">
                            {field().state.meta.errors[0]}
                          </ArkField.ErrorText>
                        </Show>
                        <ArkField.HelperText class="mt-1 text-body-small text-on-surface-variant">
                          Name displayed in Jellyfin cast menu.
                        </ArkField.HelperText>
                      </ArkField.Root>
                    )}
                  </form.Field>

                  <form.Field name="mpvPath">
                    {(field) => (
                      <ArkField.Root class="block">
                        <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          MPV executable path
                        </ArkField.Label>
                        <div class="flex flex-col gap-2 sm:flex-row">
                          <ArkField.Input
                            id={field().name}
                            name={field().name}
                            type="text"
                            value={field().state.value}
                            onInput={(event) =>
                              field().handleChange(event.currentTarget.value)
                            }
                            onBlur={(event) => {
                              field().handleBlur();
                              saveTextSetting(
                                'mpvPath',
                                event.currentTarget.value,
                              );
                            }}
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
                      </ArkField.Root>
                    )}
                  </form.Field>

                  <Collapsible.Root
                    open={advancedOpen()}
                    onClick={(event) => {
                      const target = event.target;
                      if (
                        target instanceof HTMLElement &&
                        target.closest(
                          '[data-scope="collapsible"][data-part="trigger"]',
                        )
                      ) {
                        setAdvancedOpen((open) => !open);
                      }
                    }}
                    lazyMount
                    unmountOnExit
                  >
                    <Collapsible.Trigger class="btn-text px-0">
                      <Collapsible.Indicator>
                        <ChevronDown
                          class={`h-5 w-5 transition-transform ${advancedOpen() ? 'rotate-180' : ''}`}
                        />
                      </Collapsible.Indicator>
                      Advanced MPV options
                    </Collapsible.Trigger>

                    <Collapsible.Content class="space-y-5 rounded-3xl border border-outline-variant bg-surface-container-lowest p-4">
                      <form.Field name="mpvArgs">
                        {(field) => (
                          <ArkField.Root class="block">
                            <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                              Extra arguments
                            </ArkField.Label>
                            <ArkField.Textarea
                              value={field().state.value}
                              onInput={(event) =>
                                field().handleChange(event.currentTarget.value)
                              }
                              onBlur={(event) => {
                                field().handleBlur();
                                saveTextSetting(
                                  'mpvArgs',
                                  event.currentTarget.value,
                                );
                              }}
                              rows={4}
                              placeholder="--fullscreen&#10;--force-window"
                              class="input-filled h-auto w-full py-3 font-mono text-body-small"
                            />
                          </ArkField.Root>
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
                            <ArkField.Root
                              class="block"
                              invalid={field().state.meta.errors.length > 0}
                            >
                              <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                                Next episode key
                              </ArkField.Label>
                              <ArkField.Input
                                id={field().name}
                                name={field().name}
                                type="text"
                                value={field().state.value}
                                onInput={(event) =>
                                  field().handleChange(
                                    event.currentTarget.value,
                                  )
                                }
                                onBlur={(event) => {
                                  field().handleBlur();
                                  saveTextSetting(
                                    'keybindNext',
                                    event.currentTarget.value,
                                  );
                                }}
                                class="input-filled w-full font-mono"
                                placeholder="Shift+n"
                              />
                            </ArkField.Root>
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
                            <ArkField.Root
                              class="block"
                              invalid={field().state.meta.errors.length > 0}
                            >
                              <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                                Previous episode key
                              </ArkField.Label>
                              <ArkField.Input
                                id={field().name}
                                name={field().name}
                                type="text"
                                value={field().state.value}
                                onInput={(event) =>
                                  field().handleChange(
                                    event.currentTarget.value,
                                  )
                                }
                                onBlur={(event) => {
                                  field().handleBlur();
                                  saveTextSetting(
                                    'keybindPrev',
                                    event.currentTarget.value,
                                  );
                                }}
                                class="input-filled w-full font-mono"
                                placeholder="Shift+p"
                              />
                            </ArkField.Root>
                          )}
                        </form.Field>
                      </div>
                    </Collapsible.Content>
                  </Collapsible.Root>
                  <TagsInput.Root
                    value={selectedSubtitleLanguages()}
                    inputValue={subtitleLanguageInput()}
                    onInputValueChange={(details) =>
                      setSubtitleLanguageInput(details.inputValue)
                    }
                    editable={false}
                    class="rounded-2xl bg-surface-container-high p-4"
                  >
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
                          onClick={clearPreferredSubtitleLanguages}
                        >
                          Clear all
                        </button>
                        <TagsInput.ClearTrigger class="hidden" />
                      </Show>
                    </div>

                    <Combobox.Root
                      collection={subtitleLanguageCollection()}
                      inputValue={subtitleLanguageInput()}
                      onInputValueChange={(details) =>
                        setSubtitleLanguageInput(details.inputValue)
                      }
                      onValueChange={(details) =>
                        addPreferredSubtitleLanguageCodes(details.value)
                      }
                      allowCustomValue
                      selectionBehavior="clear"
                      openOnClick
                      class="mt-4"
                    >
                      <Combobox.Control class="flex flex-col gap-3 sm:flex-row">
                        <TagsInput.Control class="min-w-0 flex-1">
                          <TagsInput.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                            Add preferred subtitle language
                          </TagsInput.Label>
                          <TagsInput.Input
                            type="text"
                            value={subtitleLanguageInput()}
                            onInput={(event) =>
                              setSubtitleLanguageInput(
                                event.currentTarget.value,
                              )
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
                        </TagsInput.Control>
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
                      </Combobox.Control>
                      <Combobox.Content class="mt-2 rounded-2xl border border-outline-variant bg-surface-container-lowest p-2 shadow-lg shadow-black/30">
                        <Combobox.List>
                          <For each={subtitleLanguageCollection().items}>
                            {(item) => (
                              <Combobox.Item
                                item={item}
                                class="cursor-pointer rounded-xl px-3 py-2 text-body-small text-on-surface-variant hover:bg-surface-container-high"
                                onClick={() =>
                                  addPreferredSubtitleLanguageCodes([
                                    item.value,
                                  ])
                                }
                              >
                                <Combobox.ItemText>
                                  {item.label}
                                </Combobox.ItemText>
                              </Combobox.Item>
                            )}
                          </For>
                        </Combobox.List>
                      </Combobox.Content>
                    </Combobox.Root>

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
                        class="mt-4 flex flex-wrap gap-2"
                        aria-label="Selected preferred subtitle languages"
                      >
                        <For each={selectedSubtitleLanguages()}>
                          {(language, index) => (
                            <TagsInput.Item
                              index={index()}
                              value={language}
                              class="inline-flex max-w-full items-center gap-2 rounded-full border border-outline-variant bg-surface-container-lowest px-3 py-2"
                            >
                              <TagsInput.ItemPreview class="contents">
                                <span class="text-label-small text-on-surface-variant">
                                  {index() + 1}
                                </span>
                                <TagsInput.ItemText class="font-mono text-label-large text-on-surface">
                                  {language}
                                </TagsInput.ItemText>
                                <span class="text-body-small text-on-surface-variant">
                                  {getSubtitleLanguageLabel(language)}
                                </span>
                              </TagsInput.ItemPreview>
                              <button
                                type="button"
                                class="btn-text min-w-0 px-1"
                                disabled={index() === 0}
                                aria-label={`Move ${language} up`}
                                onClick={() =>
                                  movePreferredSubtitleLanguage(index(), -1)
                                }
                              >
                                ↑
                              </button>
                              <button
                                type="button"
                                class="btn-text min-w-0 px-1"
                                disabled={
                                  index() ===
                                  selectedSubtitleLanguages().length - 1
                                }
                                aria-label={`Move ${language} down`}
                                onClick={() =>
                                  movePreferredSubtitleLanguage(index(), 1)
                                }
                              >
                                ↓
                              </button>
                              <TagsInput.ItemDeleteTrigger
                                class="btn-text min-w-0 px-1"
                                aria-label={`Remove ${language}`}
                                onClick={() =>
                                  removePreferredSubtitleLanguage(language)
                                }
                              >
                                Remove
                              </TagsInput.ItemDeleteTrigger>
                            </TagsInput.Item>
                          )}
                        </For>
                      </ol>
                    </Show>
                    <TagsInput.HiddenInput />
                  </TagsInput.Root>
                </div>
              </SectionCard>
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

            <SectionCard
              icon={<Bot class="h-6 w-6" />}
              title="Automatic Intro Skip"
            >
              <div class="space-y-4">
                <Checkbox.Root
                  ids={{ hiddenInput: 'intro-skipper-enabled' }}
                  name="introSkipperEnabled"
                  checked={introSkipperEnabled()}
                  onCheckedChange={(details) =>
                    handleIntroSkipperToggle(details.checked === true)
                  }
                  class="ark-checkbox flex w-full cursor-pointer items-center justify-between gap-4 rounded-2xl bg-surface-container-high px-4 py-3 text-left"
                >
                  <span>
                    <Checkbox.Label class="block text-title-medium text-on-surface">
                      Automatic Intro Skip
                    </Checkbox.Label>
                    <span class="text-body-small text-on-surface-variant">
                      Use Intro Skipper ranges when available.
                    </span>
                  </span>
                  <Checkbox.Control class="ark-checkbox__control h-6 w-6">
                    <Checkbox.Indicator class="ark-checkbox__indicator">
                      ✓
                    </Checkbox.Indicator>
                  </Checkbox.Control>
                  <Checkbox.HiddenInput aria-label="Automatic Intro Skip" />
                </Checkbox.Root>
                <Show when={introSkipperSaving()}>
                  <p class="text-body-small text-secondary">
                    Saving preference…
                  </p>
                </Show>
                <Show when={introSkipperError()}>
                  {(message) => (
                    <p class="rounded-2xl bg-error-container px-4 py-3 text-body-small text-on-error-container">
                      {message()}
                    </p>
                  )}
                </Show>
              </div>
            </SectionCard>

            <Dialog.Root
              open={confirmSignOut()}
              onOpenChange={(details) => {
                if (signingOut() && !details.open) return;
                setConfirmSignOut(details.open);
              }}
              closeOnEscape={!signingOut()}
              closeOnInteractOutside={!signingOut()}
              onEscapeKeyDown={() => {
                if (!signingOut()) setConfirmSignOut(false);
              }}
              onInteractOutside={() => {
                if (!signingOut()) setConfirmSignOut(false);
              }}
              lazyMount
              unmountOnExit
              role="dialog"
            >
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
                <Dialog.Trigger class="btn-outlined mt-5 w-full border-error/60 text-error hover:bg-error/10">
                  <LogOut class="h-5 w-5" />
                  Sign out
                </Dialog.Trigger>
              </section>

              <Portal>
                <Dialog.Backdrop
                  class="fixed inset-0 z-50 bg-black/60"
                  onClick={() => {
                    if (!signingOut()) setConfirmSignOut(false);
                  }}
                />
                <Dialog.Positioner class="fixed inset-0 z-50 flex items-center justify-center p-4">
                  <Dialog.Content
                    class="card-elevated max-w-md"
                    onKeyDown={(event) => {
                      if (event.key === 'Escape' && !signingOut()) {
                        setConfirmSignOut(false);
                      }
                    }}
                  >
                    <Dialog.Title
                      id="sign-out-title"
                      class="text-title-large text-on-surface"
                    >
                      Sign out?
                    </Dialog.Title>
                    <Dialog.Description class="mt-3 text-body-medium text-on-surface-variant">
                      This removes the Saved Session and you’ll need to
                      authenticate again before reconnecting.
                    </Dialog.Description>
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
                  </Dialog.Content>
                </Dialog.Positioner>
              </Portal>
            </Dialog.Root>

            <PageFooter />
          </aside>
        </div>
      </div>
    </div>
  );
}
