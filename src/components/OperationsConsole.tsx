import { Dialog } from '@ark-ui/solid/dialog';
import { createForm } from '@tanstack/solid-form';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Option } from 'effect';
import { X } from 'lucide-solid';
import { Show, createEffect, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';

import type { AppConfig, IntroSkipperMode } from '../bindings';
import { commandFailureMessage } from '../effects/commands';
import { detectMpv, fetchConfig, saveConfig } from '../effects/config';
import { disconnectJellyfin, fetchConnectionState } from '../effects/connection';
import {
  activateSavedServiceProfile,
  fetchSavedServiceProfiles,
  removeSavedServiceProfile,
} from '../effects/profiles';
import { queryKeys, runExit } from '../effects/query';
import { restoreSavedSession } from '../sessionAccess';
import LoginPage from './LoginPage';
import ConnectionCard from './OperationsConsole/ConnectionCard';
import DiagnosticsCard from './OperationsConsole/DiagnosticsCard';
import IntroSkipCard from './OperationsConsole/IntroSkipCard';
import LibrarySettingsCard from './OperationsConsole/LibrarySettingsCard';
import PlayerBridgeSettingsCard from './OperationsConsole/PlayerBridgeSettingsCard';
import SavedServicesCard from './OperationsConsole/SavedServicesCard';
import SessionCard from './OperationsConsole/SessionCard';
import ShortcutKeysCard from './OperationsConsole/ShortcutKeysCard';
import { createOperationsConsoleStore } from './OperationsConsole/store';
import {
  normalizePreferredSubtitleLanguages,
  parseSubtitleLanguageInput,
} from './OperationsConsole/subtitleLanguages';
import { useToast } from './ToastProvider';
import { Button, ConsoleContainer, ConsoleGrid, PageFooter } from './ui';
import type { JellyPilotSelectItem } from './ui';

import * as patterns from '../styles/patterns.css';
import * as styles from './OperationsConsole.css';

interface OperationsConsoleProps {
  onSignedOut: () => void;
}

export default function OperationsConsole(props: OperationsConsoleProps) {
  const { showToast } = useToast();
  const { state: ui, actions, Provider } = createOperationsConsoleStore();
  const [addServiceOpen, setAddServiceOpen] = createSignal(false);
  const [addServicePortalMount, setAddServicePortalMount] = createSignal<HTMLDivElement>();
  const [activatingProfileKey, setActivatingProfileKey] = createSignal<string | null>(null);
  const [removingProfileKey, setRemovingProfileKey] = createSignal<string | null>(null);
  const [imageCacheEnabledDraft, setImageCacheEnabledDraft] = createSignal<boolean | null>(null);

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
  const profilesQuery = createQuery(() => ({
    queryKey: queryKeys.savedServiceProfiles,
    queryFn: () => runExit(fetchSavedServiceProfiles()),
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
  const detectMpvMutation = createMutation(() => ({
    mutationFn: () => runExit(detectMpv()),
  }));
  const reconnectMutation = createMutation(() => ({
    mutationFn: restoreSavedSession,
  }));
  const activateProfileMutation = createMutation(() => ({
    mutationFn: (key: string) => runExit(activateSavedServiceProfile(key)),
  }));
  const removeProfileMutation = createMutation(() => ({
    mutationFn: (key: string) => runExit(removeSavedServiceProfile(key)),
  }));
  const clearLibraryQueries = () => {
    queryClient.removeQueries({ queryKey: queryKeys.libraryRoot });
  };
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
      setImageCacheEnabledDraft(null);
      form.setFieldValue('introSkipperMode', cfg.introSkipperMode ?? 'automatic');
      configHydrated = true;
    }
  });

  const state = () =>
    connectionQuery.data && Exit.isSuccess(connectionQuery.data)
      ? connectionQuery.data.value
      : undefined;
  const profiles = () =>
    profilesQuery.data && Exit.isSuccess(profilesQuery.data) ? profilesQuery.data.value : null;
  const capabilities = () => state()?.capabilities;
  const config = () =>
    configQuery.data && Exit.isSuccess(configQuery.data) ? configQuery.data.value : null;
  const introSkipperMode = () => ui.introSkipperDraft ?? config()?.introSkipperMode ?? 'automatic';
  const imageDiskCacheEnabled = () =>
    imageCacheEnabledDraft() ??
    latestConfigSnapshot?.imageDiskCacheEnabled ??
    config()?.imageDiskCacheEnabled ??
    true;

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

  const saveImageDiskCacheEnabled = (enabled: boolean) => {
    const previous = imageDiskCacheEnabled();
    const desired = latestConfigSnapshot ?? lastSavedConfig ?? config();
    if (desired?.imageDiskCacheEnabled === enabled) {
      return;
    }

    setImageCacheEnabledDraft(enabled);
    queueConfigSave(buildConfigSnapshot({ imageDiskCacheEnabled: enabled }), {
      onError: () => {
        setImageCacheEnabledDraft(previous);
      },
      onSuccess: () => {
        setImageCacheEnabledDraft(null);
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
    if (!profiles()?.activeProfileKey) {
      showToast('error', 'No active saved service is available. Choose a saved service.');
      return;
    }

    actions.beginReconnect();
    try {
      if (await reconnectMutation.mutateAsync()) {
        clearLibraryQueries();
        showToast('success', 'Reconnected to saved service');
        void connectionQuery.refetch();
        void profilesQuery.refetch();
      } else {
        showToast('error', 'Could not reconnect to the saved service.');
        void profilesQuery.refetch();
      }
    } finally {
      actions.finishReconnect();
    }
  };

  const handleDisconnect = async () => {
    actions.beginDisconnect();
    const exit = await disconnectMutation.mutateAsync();
    if (Exit.isSuccess(exit)) {
      clearLibraryQueries();
      showToast('success', 'Disconnected from Jellyfin');
      void connectionQuery.refetch();
    } else {
      showToast('error', commandFailureMessage(exit.cause, 'Disconnect failed'));
    }
    actions.finishDisconnect();
  };

  const handleSignOut = async () => {
    const activeProfileKey = profiles()?.activeProfileKey;
    if (!activeProfileKey) {
      props.onSignedOut();
      return;
    }

    actions.beginSignOut();
    setRemovingProfileKey(activeProfileKey);
    try {
      const exit = await removeProfileMutation.mutateAsync(activeProfileKey);
      if (Exit.isSuccess(exit)) {
        clearLibraryQueries();
        void connectionQuery.refetch();
        void profilesQuery.refetch();
        if (exit.value.profiles.length === 0) {
          props.onSignedOut();
        } else {
          showToast('success', 'Signed out of the active service. Choose another saved service.');
        }
      } else {
        showToast('error', commandFailureMessage(exit.cause, 'Sign out failed'));
      }
    } finally {
      setRemovingProfileKey(null);
      actions.finishSignOut();
    }
  };

  const handleActivateProfile = async (key: string) => {
    setActivatingProfileKey(key);
    try {
      const exit = await activateProfileMutation.mutateAsync(key);
      if (Exit.isSuccess(exit)) {
        clearLibraryQueries();
        showToast('success', 'Switched active service');
        void connectionQuery.refetch();
        void profilesQuery.refetch();
      } else {
        showToast('error', commandFailureMessage(exit.cause, 'Could not switch service'));
        void profilesQuery.refetch();
      }
    } finally {
      setActivatingProfileKey(null);
    }
  };

  const handleRemoveProfile = async (key: string) => {
    setRemovingProfileKey(key);
    try {
      const exit = await removeProfileMutation.mutateAsync(key);
      if (Exit.isSuccess(exit)) {
        if (profiles()?.activeProfileKey === key) {
          clearLibraryQueries();
        }
        void connectionQuery.refetch();
        void profilesQuery.refetch();
        if (exit.value.profiles.length === 0) {
          props.onSignedOut();
        } else {
          showToast('success', 'Saved service removed');
        }
      } else {
        showToast('error', commandFailureMessage(exit.cause, 'Could not remove saved service'));
      }
    } finally {
      setRemovingProfileKey(null);
    }
  };

  const handleAddServiceConnected = () => {
    clearLibraryQueries();
    setAddServiceOpen(false);
    showToast('success', 'Saved service added and activated');
    void connectionQuery.refetch();
    void profilesQuery.refetch();
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
          <div class={styles.stack}>
            <SavedServicesCard
              profiles={profiles()}
              activatingProfileKey={activatingProfileKey()}
              removingProfileKey={removingProfileKey()}
              onAddService={() => setAddServiceOpen(true)}
              onActivateProfile={handleActivateProfile}
              onRemoveProfile={handleRemoveProfile}
            />

            <ConnectionCard
              state={state()}
              canReconnect={Boolean(profiles()?.activeProfileKey)}
              onDisconnect={handleDisconnect}
              onReconnect={handleReconnect}
              onRefresh={handleRefresh}
            />

            <form class={styles.stack}>
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

          <aside class={styles.stack}>
            <DiagnosticsCard />

            <LibrarySettingsCard
              imageDiskCacheEnabled={imageDiskCacheEnabled()}
              onImageDiskCacheEnabledChange={saveImageDiskCacheEnabled}
            />

            <Show when={capabilities()?.introSkipper ?? true}>
              <IntroSkipCard
                currentMode={introSkipperMode()}
                onModeChange={handleIntroSkipperModeChange}
              />
            </Show>
            <ShortcutKeysCard
              form={form}
              showIntroSkipKey={capabilities()?.introSkipper ?? true}
              onSaveTextSetting={saveTextSetting}
            />

            <SessionCard onSignOut={handleSignOut} />

            <PageFooter />
          </aside>
        </ConsoleGrid>
      </ConsoleContainer>
      <div ref={setAddServicePortalMount} />
      <Dialog.Root
        open={addServiceOpen()}
        onOpenChange={(details) => setAddServiceOpen(details.open)}
        lazyMount
        unmountOnExit
      >
        <Portal mount={addServicePortalMount()}>
          <Dialog.Backdrop class={styles.backdrop} />
          <Dialog.Positioner class={`${styles.positioner} ${styles.positionerFill}`}>
            <Dialog.Content class={styles.content}>
              <Dialog.Title class={patterns.srOnly}>Add saved service</Dialog.Title>
              <Dialog.Description class={patterns.srOnly}>
                Log in to a Jellyfin or Emby service and save it for switching.
              </Dialog.Description>
              <Button
                type="button"
                variant="icon"
                class={styles.closeButton}
                aria-label="Close add service"
                title="Close add service"
                onClick={() => setAddServiceOpen(false)}
              >
                <X class={patterns.icon4_5} />
              </Button>
              <LoginPage embedded onConnected={handleAddServiceConnected} />
            </Dialog.Content>
          </Dialog.Positioner>
        </Portal>
      </Dialog.Root>
    </Provider>
  );
}
