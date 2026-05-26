import { Collapsible } from '@ark-ui/solid/collapsible';
import { Combobox, createListCollection } from '@ark-ui/solid/combobox';
import { Dialog } from '@ark-ui/solid/dialog';
import { Field as ArkField } from '@ark-ui/solid/field';
import { TagsInput } from '@ark-ui/solid/tags-input';
import { createForm } from '@tanstack/solid-form';
import { Effect, Exit } from 'effect';
import {
  Activity,
  Bot,
  ChevronDown,
  ClipboardList,
  LogOut,
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
import {
  type AppConfig,
  type ConnectionState,
  commands,
  type IntroSkipperMode,
} from '../bindings';
import {
  commandFailure,
  commandFailureMessage,
  runTauriCommand,
  runTauriCommandRaw,
} from '../effects/commands';
import { detectMpv } from '../effects/config';
import {
  clearSavedSession,
  loadSavedSession,
  restoreSavedSession,
} from '../sessionAccess';
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

const INTRO_SKIPPER_MODES: {
  mode: IntroSkipperMode;
  label: string;
  description: string;
}[] = [
  {
    mode: 'automatic',
    label: 'Automatic',
    description: 'Skip ranges as playback reaches them.',
  },
  {
    mode: 'manual',
    label: 'Manual',
    description: 'Show an MPV prompt and wait for the shortcut.',
  },
  {
    mode: 'off',
    label: 'Off',
    description: 'Do not fetch or apply Intro Skipper ranges.',
  },
];

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
  const [introSkipperDraft, setIntroSkipperDraft] =
    createSignal<IntroSkipperMode | null>(null);
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
  const [_mpvConnected, { refetch: refetchMpv }] =
    createResource(fetchMpvStatus);
  const [initialConfig, { mutate: mutateConfig }] = createResource(async () => {
    const exit = await Effect.runPromiseExit(
      runTauriCommandRaw(() => commands.configGet()),
    );
    if (Exit.isSuccess(exit)) return exit.value;

    console.error(
      'Failed to load config:',
      commandFailureMessage(exit.cause, 'Could not load configuration'),
    );
    return null;
  });

  const form = createForm(() => ({
    defaultValues: {
      deviceName: 'JMSR',
      mpvPath: '',
      mpvArgs: '',
      keybindNext: 'Shift+n',
      keybindPrev: 'Shift+p',
      keybindIntroSkip: 'g',
      introSkipperMode: 'automatic' as IntroSkipperMode,
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
      form.setFieldValue('keybindIntroSkip', cfg.keybindIntroSkip ?? 'g');
      setSelectedSubtitleLanguages(
        normalizePreferredSubtitleLanguages(cfg.preferredSubtitleLanguages),
      );
      form.setFieldValue(
        'introSkipperMode',
        cfg.introSkipperMode ?? 'automatic',
      );
      if ((cfg.mpvArgs?.length ?? 0) > 0) setAdvancedOpen(true);
      configHydrated = true;
    }
  });

  const state = () => connectionState();
  const config = () => initialConfig();
  const introSkipperMode = () =>
    introSkipperDraft() ?? config()?.introSkipperMode ?? 'automatic';
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

        const exit = await Effect.runPromiseExit(
          runTauriCommand(() => commands.configSet(nextSave.config)),
        );
        if (Exit.isSuccess(exit)) {
          lastSavedConfig = nextSave.config;
          mutateConfig(nextSave.config);
          refetchMpv();
          nextSave.onSuccess?.();
          showPlayerBridgeStatus('saved', 'Saved');
        } else {
          const message = commandFailureMessage(
            exit.cause,
            'Could not save configuration',
          );
          nextSave.onError?.(message);
          showPlayerBridgeStatus('error', message);
          showToast('error', message);
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
    field:
      | 'deviceName'
      | 'mpvPath'
      | 'mpvArgs'
      | 'keybindNext'
      | 'keybindPrev'
      | 'keybindIntroSkip',
    value: string,
  ) => {
    const saved = lastSavedConfig ?? config();
    const desired = latestConfigSnapshot ?? saved;
    if (!saved || !desired) return;

    if (field === 'deviceName' && value.trim().length === 0) return;
    if (field === 'keybindNext' && value.trim().length === 0) return;
    if (field === 'keybindPrev' && value.trim().length === 0) return;
    if (field === 'keybindIntroSkip' && value.trim().length === 0) return;

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
    } else if (value === desired[field]) {
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

  const saveIntroSkipperSetting = (mode: IntroSkipperMode) => {
    const previous = introSkipperMode();
    const desired = latestConfigSnapshot ?? lastSavedConfig ?? config();
    if (desired?.introSkipperMode === mode) return;

    setIntroSkipperDraft(mode);
    setIntroSkipperSaving(true);
    setIntroSkipperError(null);
    queueConfigSave(buildConfigSnapshot({ introSkipperMode: mode }), {
      onSuccess: () => {
        setIntroSkipperDraft(null);
        setIntroSkipperSaving(false);
      },
      onError: (message) => {
        setIntroSkipperDraft(previous);
        setIntroSkipperSaving(false);
        setIntroSkipperError(message);
        form.setFieldValue('introSkipperMode', previous);
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
      if (await restoreSavedSession()) {
        showToast('success', 'Reconnected to Jellyfin');
        refetchConnection();
      } else {
        showToast('error', 'Could not reconnect to Jellyfin. Sign in again.');
        props.onSignedOut();
      }
    } finally {
      setReconnecting(false);
    }
  };

  const handleDisconnect = async () => {
    setDisconnecting(true);
    const exit = await Effect.runPromiseExit(
      runTauriCommand(() => commands.jellyfinDisconnect()),
    );
    if (Exit.isSuccess(exit)) {
      showToast('success', 'Disconnected from Jellyfin');
      refetchConnection();
    } else {
      showToast(
        'error',
        commandFailureMessage(exit.cause, 'Disconnect failed'),
      );
    }
    setDisconnecting(false);
  };

  const handleSignOut = async () => {
    setSigningOut(true);
    const exit = await Effect.runPromiseExit(
      runTauriCommand(() => commands.jellyfinClearSession()),
    );
    if (Exit.isSuccess(exit)) {
      clearSavedSession();
      props.onSignedOut();
    } else {
      showToast('error', commandFailureMessage(exit.cause, 'Sign out failed'));
    }
    setSigningOut(false);
    setConfirmSignOut(false);
  };

  const handleDetectMpv = async () => {
    setDetectingMpv(true);
    const exit = await Effect.runPromiseExit(detectMpv());
    if (Exit.isSuccess(exit)) {
      const path = exit.value;
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
    } else {
      console.error(
        'Failed to detect MPV:',
        commandFailure(exit.cause) ??
          commandFailureMessage(exit.cause, 'Failed to detect MPV'),
      );
      showToast('error', 'Failed to detect MPV');
    }
    setDetectingMpv(false);
  };

  const handleIntroSkipperModeChange = (mode: IntroSkipperMode) => {
    form.setFieldValue('introSkipperMode', mode);
    saveIntroSkipperSetting(mode);
  };

  return (
    <div class="console-shell">
      <div class="console-container">
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
                <button
                  type="button"
                  onClick={handleRefresh}
                  class="btn-icon ml-auto"
                  aria-label="Refresh status"
                  title="Refresh status"
                >
                  <RefreshCw class="h-5 w-5" />
                </button>
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
                    onOpenChange={(details) => setAdvancedOpen(details.open)}
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

                      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
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
                        <form.Field
                          name="keybindIntroSkip"
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
                                Intro skip key
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
                                    'keybindIntroSkip',
                                    event.currentTarget.value,
                                  );
                                }}
                                class="input-filled w-full font-mono"
                                placeholder="g"
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
                          <Combobox.Input
                            autocomplete="off"
                            asChild={(comboboxInputProps) => (
                              <TagsInput.Input
                                {...comboboxInputProps({
                                  type: 'text',
                                  value: subtitleLanguageInput(),
                                  onInput: (event) =>
                                    setSubtitleLanguageInput(
                                      event.currentTarget.value,
                                    ),
                                  onKeyDown: (event) => {
                                    if (event.key !== 'Enter') return;
                                    event.preventDefault();
                                    addPreferredSubtitleLanguages();
                                  },
                                  class: 'input-filled w-full font-mono',
                                  placeholder: 'eng',
                                  'aria-label':
                                    'Add preferred subtitle language',
                                })}
                              />
                            )}
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

            <SectionCard icon={<Bot class="h-6 w-6" />} title="Intro Skip">
              <div class="space-y-4">
                <fieldset
                  class="grid grid-cols-1 gap-3"
                  aria-label="Intro Skip Mode"
                >
                  <For each={INTRO_SKIPPER_MODES}>
                    {(option) => (
                      <button
                        type="button"
                        class={`rounded-2xl border px-4 py-3 text-left transition ${
                          introSkipperMode() === option.mode
                            ? 'border-primary bg-primary-container text-on-primary-container'
                            : 'border-outline-variant bg-surface-container-high text-on-surface hover:border-primary/50'
                        }`}
                        aria-pressed={introSkipperMode() === option.mode}
                        onClick={() =>
                          handleIntroSkipperModeChange(option.mode)
                        }
                      >
                        <span class="block text-title-medium">
                          {option.label}
                        </span>
                        <span class="mt-1 block text-body-small opacity-80">
                          {option.description}
                        </span>
                      </button>
                    )}
                  </For>
                </fieldset>
                <Show when={introSkipperSaving()}>
                  <p class="text-body-small text-secondary">
                    Saving preference…
                  </p>
                </Show>
                <p class="text-body-small text-on-surface-variant">
                  Changes take effect after restarting MPV.
                </p>
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
