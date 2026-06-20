import { createForm } from '@tanstack/solid-form';
import { Effect, Exit } from 'effect';
import { createEffect, createResource } from 'solid-js';

import { commands } from '../bindings';
import type { AppConfig, ConnectionState, IntroSkipperMode } from '../bindings';
import { commandFailureMessage, runTauriCommand, runTauriCommandRaw } from '../effects/commands';
import { detectMpv } from '../effects/config';
import { clearSavedSession, loadSavedSession, restoreSavedSession } from '../sessionAccess';
import NowPlayingCard from './NowPlayingCard';
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
import type { JmsrSelectItem } from './ui';

interface OperationsConsoleProps {
  onSignedOut: () => void;
}

async function fetchConnectionState(): Promise<ConnectionState> {
  return await commands.jellyfinGetState();
}

async function fetchMpvStatus(): Promise<boolean> {
  return await commands.mpvIsConnected();
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

  const subtitleLanguageSelectItems: JmsrSelectItem[] = [
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

  const [connectionState, { refetch: refetchConnection }] = createResource(fetchConnectionState);
  const [_mpvConnected, { refetch: refetchMpv }] = createResource(fetchMpvStatus);
  const [initialConfig, { mutate: mutateConfig }] = createResource(async () => {
    const exit = await Effect.runPromiseExit(runTauriCommandRaw(() => commands.configGet()));
    if (Exit.isSuccess(exit)) {
      return exit.value;
    }

    console.error(
      'Failed to load config:',
      commandFailureMessage(exit.cause, 'Could not load configuration'),
    );
    return null;
  });

  const form = createForm(() => ({
    defaultValues: {
      deviceName: 'JMSR',
      introSkipperMode: 'automatic' as IntroSkipperMode,
      keybindIntroSkip: 'g',
      keybindNext: 'Shift+>',
      keybindPrev: 'Shift+<',
      mpvArgs: '',
      mpvPath: '',
    },
  }));

  createEffect(() => {
    const cfg = initialConfig();
    if (cfg && !configHydrated) {
      lastSavedConfig = cfg;
      form.setFieldValue('deviceName', cfg.deviceName ?? 'JMSR');
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

  const state = () => connectionState();
  const config = () => initialConfig();
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

    actions.beginReconnect();
    try {
      if (await restoreSavedSession()) {
        showToast('success', 'Reconnected to Jellyfin');
        refetchConnection();
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
    const exit = await Effect.runPromiseExit(runTauriCommand(() => commands.jellyfinDisconnect()));
    if (Exit.isSuccess(exit)) {
      showToast('success', 'Disconnected from Jellyfin');
      refetchConnection();
    } else {
      showToast('error', commandFailureMessage(exit.cause, 'Disconnect failed'));
    }
    actions.finishDisconnect();
  };

  const handleSignOut = async () => {
    actions.beginSignOut();
    const exit = await Effect.runPromiseExit(
      runTauriCommand(() => commands.jellyfinClearSession()),
    );
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
    const exit = await Effect.runPromiseExit(detectMpv());
    if (Exit.isSuccess(exit)) {
      const path = exit.value;
      if (path) {
        form.setFieldValue('mpvPath', path);
        queueConfigSave(buildConfigSnapshot({ mpvPath: path }));
        showToast('success', 'MPV detected successfully');
      } else {
        showToast('warning', 'MPV not found in PATH. Configure the path manually.');
      }
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

            <NowPlayingCard
              jellyfinConnected={state()?.connected ?? false}
              onPlayerStarted={() => refetchMpv()}
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
