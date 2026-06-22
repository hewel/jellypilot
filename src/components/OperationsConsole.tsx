import { createForm } from '@tanstack/solid-form';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Option } from 'effect';
import { createEffect } from 'solid-js';

import type { AppConfig, IntroSkipperMode } from '../bindings';
import { commandFailureMessage } from '../effects/commands';
import { detectMpv, fetchConfig, saveConfig } from '../effects/config';
import {
  clearJellyfinSession,
  disconnectJellyfin,
  fetchConnectionState,
} from '../effects/connection';
import { queryKeys, runExit } from '../effects/query';
import { clearSavedSession, loadSavedSession, restoreSavedSession } from '../sessionAccess';
import ConnectionCard from './OperationsConsole/ConnectionCard';
import DiagnosticsCard from './OperationsConsole/DiagnosticsCard';
import IntroSkipCard from './OperationsConsole/IntroSkipCard';
import PlayerBridgeSettingsCard from './OperationsConsole/PlayerBridgeSettingsCard';
import SessionCard from './OperationsConsole/SessionCard';
import ShortcutKeysCard from './OperationsConsole/ShortcutKeysCard';
import { createOperationsConsoleStore } from './OperationsConsole/store';
import {
  normalizePreferredSubtitleLanguages,
  parseSubtitleLanguageInput,
} from './OperationsConsole/subtitleLanguages';
import { useToast } from './ToastProvider';
import { ConsoleContainer, ConsoleGrid, PageFooter } from './ui';
import type { JellyPilotSelectItem } from './ui';

interface OperationsConsoleProps {
  onSignedOut: () => void;
}

export default function OperationsConsole(props: OperationsConsoleProps) {
  const { showToast } = useToast();
  const { state: ui, actions, Provider } = createOperationsConsoleStore();

  let configHydrated = false;
  interface PendingSave {
    config: AppConfig;
    onSuccess?: () => void;
    onError?: (message: string) => void;
  }
  let lastSavedConfig: AppConfig | null = null;
  let saveInFlight = false;
  let pendingSave: PendingSave | null = null;
  let latestConfigSnapshot: AppConfig | null = null;
  let clearPlayerBridgeStatusTimer: ReturnType<typeof setTimeout> | null = null;

  const subtitleLanguageSelectItems: JellyPilotSelectItem[] = [
    { label: 'eng — English', value: 'eng' },
    { label: 'jpn — Japanese', value: 'jpn' },
    { label: 'spa — Spanish', value: 'spa' },
    { label: 'fre — French', value: 'fre' },
    { label: 'ger — German', value: 'ger' },
    { label: 'ita — Italian', value: 'ita' },
    { label: 'por — Portuguese', value: 'por' },
    { label: 'chi — Chinese', value: 'chi' },
    { label: 'kor — Korean', value: 'kor' },
  ];

  const queryClient = useQueryClient();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState()),
  }));
  const configQuery = createQuery(() => ({
    queryKey: queryKeys.appConfig,
    queryFn: () => runExit(fetchConfig()),
  }));
  const saveConfigMutation = createMutation(() => ({
    mutationFn: (config: AppConfig) => runExit(saveConfig(config)),
  }));
  const disconnectMutation = createMutation(() => ({
    mutationFn: () => runExit(disconnectJellyfin()),
  }));
  const clearSessionMutation = createMutation(() => ({
    mutationFn: () => runExit(clearJellyfinSession()),
  }));
  const detectMpvMutation = createMutation(() => ({
    mutationFn: () => runExit(detectMpv()),
  }));
  const reconnectMutation = createMutation(() => ({
    mutationFn: restoreSavedSession,
  }));
  let loggedConfigFailure: string | null = null;
  createEffect(() => {
    const result = configQuery.data;
    if (!result || Exit.isSuccess(result)) return;
    const message = commandFailureMessage(result.cause, 'Could not load configuration');
    if (message === loggedConfigFailure) return;
    loggedConfigFailure = message;
    console.error('Failed to load config:', message);
  });

  const form = createForm(() => ({
    defaultValues: {
      deviceName: 'JellyPilot',
      introSkipperMode: 'automatic' as IntroSkipperMode,
      keybindIntroSkip: 'g',
      keybindNext: 'Shift+>',
      keybindPrev: 'Shift+<',
      mpvArgs: '',
      mpvPath: '',
    },
  }));

  createEffect(() => {
    const cfg = config();
    if (cfg && !configHydrated) {
      lastSavedConfig = cfg;
      form.setFieldValue('deviceName', cfg.deviceName ?? 'JellyPilot');
      form.setFieldValue('mpvPath', cfg.mpvPath ?? '');
      form.setFieldValue('mpvArgs', (cfg.mpvArgs ?? []).join('\n'));
      form.setFieldValue('keybindNext', cfg.keybindNext ?? 'Shift+>');
      form.setFieldValue('keybindPrev', cfg.keybindPrev ?? 'Shift+<');
      form.setFieldValue('keybindIntroSkip', cfg.keybindIntroSkip ?? 'g');
      actions.hydrateFromConfig({
        introSkipperMode: cfg.introSkipperMode ?? 'automatic',
        mpvArgs: cfg.mpvArgs,
        preferredSubtitleLanguages: normalizePreferredSubtitleLanguages(
          cfg.preferredSubtitleLanguages,
        ),
      });
      form.setFieldValue('introSkipperMode', cfg.introSkipperMode ?? 'automatic');
      configHydrated = true;
    }
  });

  const state = () =>
    connectionQuery.data && Exit.isSuccess(connectionQuery.data)
      ? connectionQuery.data.value
      : undefined;
  const config = () =>
    configQuery.data && Exit.isSuccess(configQuery.data) ? configQuery.data.value : null;
  const introSkipperMode = () => ui.introSkipperDraft ?? config()?.introSkipperMode ?? 'automatic';

  const showPlayerBridgeStatus = (type: 'saving' | 'saved' | 'error', text: string) => {
    if (clearPlayerBridgeStatusTimer) {
      clearTimeout(clearPlayerBridgeStatusTimer);
      clearPlayerBridgeStatusTimer = null;
    }
    actions.showPlayerBridgeStatus({ text, type });
    if (type === 'saved') {
      clearPlayerBridgeStatusTimer = setTimeout(() => actions.clearPlayerBridgeStatus(), 3000);
    }
  };

  const parseMpvArgs = (value: string) =>
    value
      .split('\n')
      .map((arg) => arg.trim())
      .filter((arg) => arg.length > 0);

  const buildConfigSnapshot = (overrides: Partial<AppConfig>) => {
    const saved = pendingSave?.config ?? latestConfigSnapshot ?? lastSavedConfig ?? config();
    if (!saved) {
      return null;
    }

    return {
      ...saved,
      ...overrides,
    };
  };

  const processConfigSaveQueue = async () => {
    if (saveInFlight) {
      return;
    }
    saveInFlight = true;

    try {
      while (pendingSave) {
        const nextSave = pendingSave;
        pendingSave = null;
        showPlayerBridgeStatus('saving', 'Saving…');

        const exit = await saveConfigMutation.mutateAsync(nextSave.config);
        if (Exit.isSuccess(exit)) {
          lastSavedConfig = nextSave.config;
          queryClient.setQueryData(queryKeys.appConfig, Exit.succeed(nextSave.config));
          nextSave.onSuccess?.();
          showPlayerBridgeStatus('saved', 'Saved');
        } else {
          const message = commandFailureMessage(exit.cause, 'Could not save configuration');
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
      if (pendingSave) {
        void processConfigSaveQueue();
      }
    }
  };

  const queueConfigSave = (
    snapshot: AppConfig | null,
    callbacks: Omit<PendingSave, 'config'> = {},
  ) => {
    if (!snapshot) {
      return;
    }
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
    if (!saved || !desired) {
      return;
    }

    if (field === 'deviceName' && value.trim().length === 0) {
      return;
    }
    if (field === 'keybindNext' && value.trim().length === 0) {
      return;
    }
    if (field === 'keybindPrev' && value.trim().length === 0) {
      return;
    }
    if (field === 'keybindIntroSkip' && value.trim().length === 0) {
      return;
    }

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
      ) {
        return;
      }
    } else if (field === 'mpvPath') {
      if (override.mpvPath === desired.mpvPath) {
        return;
      }
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
      languages.every((language, index) => language === desired.preferredSubtitleLanguages?.[index])
    ) {
      return;
    }

    queueConfigSave(buildConfigSnapshot({ preferredSubtitleLanguages: languages }));
  };

  const saveIntroSkipperSetting = (mode: IntroSkipperMode) => {
    const previous = introSkipperMode();
    const desired = latestConfigSnapshot ?? lastSavedConfig ?? config();
    if (desired?.introSkipperMode === mode) {
      return;
    }

    actions.beginIntroSkipperSave(mode);
    queueConfigSave(buildConfigSnapshot({ introSkipperMode: mode }), {
      onError: (message) => {
        actions.failIntroSkipperSave(previous, message);
        form.setFieldValue('introSkipperMode', previous);
      },
      onSuccess: () => {
        actions.finishIntroSkipperSave();
      },
    });
  };

  const addPreferredSubtitleLanguageCodes = (languages: string[]) => {
    if (languages.length === 0) {
      return;
    }

    const current = ui.selectedSubtitleLanguages;
    const seen = new Set(current);
    const next = [...current];

    for (const language of languages) {
      const [code] = parseSubtitleLanguageInput(language);
      if (!code || seen.has(code)) {
        continue;
      }
      seen.add(code);
      next.push(code);
    }

    actions.setPreferredSubtitleLanguages(next);
    savePreferredSubtitleLanguages(next);
    actions.setSubtitleLanguageInput('');
  };

  const addPreferredSubtitleLanguages = () => {
    addPreferredSubtitleLanguageCodes(parseSubtitleLanguageInput(ui.subtitleLanguageInput));
  };
  const removePreferredSubtitleLanguage = (language: string) => {
    const next = ui.selectedSubtitleLanguages.filter((selected) => selected !== language);
    actions.setPreferredSubtitleLanguages(next);
    savePreferredSubtitleLanguages(next);
  };

  const clearPreferredSubtitleLanguages = () => {
    actions.setPreferredSubtitleLanguages([]);
    savePreferredSubtitleLanguages([]);
  };

  const movePreferredSubtitleLanguage = (index: number, direction: -1 | 1) => {
    const current = ui.selectedSubtitleLanguages;
    const target = index + direction;
    if (target < 0 || target >= current.length) {
      return;
    }

    const next = [...current];
    [next[index], next[target]] = [next[target], next[index]];
    actions.setPreferredSubtitleLanguages(next);
    savePreferredSubtitleLanguages(next);
  };

  const handleRefresh = () => {
    void connectionQuery.refetch();
  };

  const handleReconnect = async () => {
    const session = loadSavedSession();
    if (!session) {
      showToast('error', 'No Saved Session is available. Sign in again.');
      props.onSignedOut();
      return;
    }

    actions.beginReconnect();
    try {
      if (await reconnectMutation.mutateAsync()) {
        showToast('success', 'Reconnected to Jellyfin');
        void connectionQuery.refetch();
      } else {
        showToast('error', 'Could not reconnect to Jellyfin. Sign in again.');
        props.onSignedOut();
      }
    } finally {
      actions.finishReconnect();
    }
  };

  const handleDisconnect = async () => {
    actions.beginDisconnect();
    const exit = await disconnectMutation.mutateAsync();
    if (Exit.isSuccess(exit)) {
      showToast('success', 'Disconnected from Jellyfin');
      void connectionQuery.refetch();
    } else {
      showToast('error', commandFailureMessage(exit.cause, 'Disconnect failed'));
    }
    actions.finishDisconnect();
  };

  const handleSignOut = async () => {
    actions.beginSignOut();
    const exit = await clearSessionMutation.mutateAsync();
    if (Exit.isSuccess(exit)) {
      clearSavedSession();
      props.onSignedOut();
    } else {
      showToast('error', commandFailureMessage(exit.cause, 'Sign out failed'));
    }
    actions.finishSignOut();
  };

  const handleDetectMpv = async () => {
    actions.beginMpvDetection();
    const exit = await detectMpvMutation.mutateAsync();
    if (Exit.isSuccess(exit)) {
      Option.match(exit.value, {
        onNone: () => showToast('warning', 'MPV not found in PATH. Configure the path manually.'),
        onSome: (path) => {
          form.setFieldValue('mpvPath', path);
          queueConfigSave(buildConfigSnapshot({ mpvPath: path }));
          showToast('success', 'MPV detected successfully');
        },
      });
    } else {
      console.error(
        'Failed to detect MPV:',
        commandFailureMessage(exit.cause, 'Failed to detect MPV'),
      );
      showToast('error', 'Failed to detect MPV');
    }
    actions.finishMpvDetection();
  };

  const handleIntroSkipperModeChange = (mode: IntroSkipperMode) => {
    form.setFieldValue('introSkipperMode', mode);
    saveIntroSkipperSetting(mode);
  };

  return (
    <Provider>
      <ConsoleContainer>
        <ConsoleGrid>
          <div class="space-y-6">
            <ConnectionCard
              state={state()}
              canReconnect={Boolean(loadSavedSession())}
              onDisconnect={handleDisconnect}
              onReconnect={handleReconnect}
              onRefresh={handleRefresh}
            />

            <form class="space-y-6">
              <PlayerBridgeSettingsCard
                form={form}
                subtitleLanguageSelectItems={subtitleLanguageSelectItems}
                onSaveTextSetting={(field, value) => {
                  if (field === 'deviceName' || field === 'mpvPath' || field === 'mpvArgs') {
                    saveTextSetting(field, value);
                  }
                }}
                onDetectMpv={handleDetectMpv}
                onAddSubtitleLanguageCodes={addPreferredSubtitleLanguageCodes}
                onAddSubtitleLanguages={addPreferredSubtitleLanguages}
                onRemoveSubtitleLanguage={removePreferredSubtitleLanguage}
                onClearSubtitleLanguages={clearPreferredSubtitleLanguages}
                onMoveSubtitleLanguage={movePreferredSubtitleLanguage}
              />
            </form>
          </div>

          <aside class="space-y-6">
            <DiagnosticsCard />

            <IntroSkipCard
              currentMode={introSkipperMode()}
              onModeChange={handleIntroSkipperModeChange}
            />
            <ShortcutKeysCard form={form} onSaveTextSetting={saveTextSetting} />

            <SessionCard onSignOut={handleSignOut} />

            <PageFooter />
          </aside>
        </ConsoleGrid>
      </ConsoleContainer>
    </Provider>
  );
}
