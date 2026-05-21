import { createForm } from '@tanstack/solid-form';
import {
  Cast,
  ChevronDown,
  CircleCheckBig,
  ClipboardList,
  Keyboard,
  LogOut,
  Play,
  RefreshCw,
  SkipForward,
  Trash2,
} from 'lucide-solid';
import { createEffect, createResource, createSignal, Show } from 'solid-js';
import { type AppConfig, type ConnectionState, commands } from '../bindings';
import { clearSavedSession } from '../router';
import DiagnosticsPanel from './DiagnosticsPanel';
import { useToast } from './ToastProvider';
import {
  InfoCard,
  PageFooter,
  PageHeader,
  SectionCard,
  StatusBadge,
  StatusIndicator,
} from './ui';

interface SettingsPageProps {
  onDisconnected: () => void;
}

async function fetchConnectionState(): Promise<ConnectionState> {
  return await commands.jellyfinGetState();
}

async function fetchMpvStatus(): Promise<boolean> {
  return await commands.mpvIsConnected();
}

export default function SettingsPage(props: SettingsPageProps) {
  const { showToast } = useToast();
  let introSkipperEnabledValue = true;
  let configHydrated = false;
  let introSkipperInput: HTMLInputElement | undefined;
  const [disconnecting, setDisconnecting] = createSignal(false);
  const [clearingSession, setClearingSession] = createSignal(false);
  const [detectingMpv, setDetectingMpv] = createSignal(false);
  const [troubleshootingExpanded, setTroubleshootingExpanded] =
    createSignal(false);
  const [saveMessage, setSaveMessage] = createSignal<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);

  const [connectionState, { refetch: refetchConnection }] =
    createResource(fetchConnectionState);
  const [mpvConnected, { refetch: refetchMpv }] =
    createResource(fetchMpvStatus);

  // Load initial configuration
  const [initialConfig] = createResource(async () => {
    try {
      return await commands.configGet();
    } catch (e) {
      console.error('Failed to load config:', e);
      return null;
    }
  });

  // Create form with @tanstack/solid-form
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

        const newConfig: AppConfig = {
          deviceName: value.deviceName,
          mpvPath: value.mpvPath || null,
          mpvArgs: argsList,
          progressInterval: cfg?.progressInterval ?? 5,
          startMinimized: cfg?.startMinimized ?? false,
          introSkipperEnabled:
            introSkipperInput?.checked ?? introSkipperEnabledValue,
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
        } else {
          setSaveMessage({ type: 'error', text: result.error.message });
        }
      } catch (e) {
        setSaveMessage({ type: 'error', text: String(e) });
      }
    },
  }));

  // Update form values when config is loaded
  createEffect(() => {
    const cfg = initialConfig();
    if (cfg && !configHydrated) {
      form.setFieldValue('deviceName', cfg.deviceName ?? 'JMSR');
      form.setFieldValue('mpvPath', cfg.mpvPath ?? '');
      form.setFieldValue('mpvArgs', (cfg.mpvArgs ?? []).join('\n'));
      form.setFieldValue('keybindNext', cfg.keybindNext ?? 'Shift+n');
      form.setFieldValue('keybindPrev', cfg.keybindPrev ?? 'Shift+p');
      form.setFieldValue(
        'introSkipperEnabled',
        cfg.introSkipperEnabled ?? true,
      );
      introSkipperEnabledValue = cfg.introSkipperEnabled ?? true;
      if (introSkipperInput) {
        introSkipperInput.checked = cfg.introSkipperEnabled ?? true;
      }
      configHydrated = true;
    }
  });

  const handleDisconnect = async () => {
    setDisconnecting(true);
    try {
      const result = await commands.jellyfinDisconnect();
      if (result.status === 'ok') {
        clearSavedSession();
        props.onDisconnected();
      }
    } finally {
      setDisconnecting(false);
    }
  };

  const handleClearSession = async () => {
    setClearingSession(true);
    try {
      await commands.jellyfinClearSession();
      clearSavedSession();
      props.onDisconnected();
    } finally {
      setClearingSession(false);
    }
  };

  const handleRefresh = () => {
    refetchConnection();
    refetchMpv();
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
          'MPV not found in PATH. Please specify path manually.',
        );
      }
    } catch (e) {
      console.error('Failed to detect MPV:', e);
      showToast('error', 'Failed to detect MPV');
    } finally {
      setDetectingMpv(false);
    }
  };

  const handleIntroSkipperToggle = (enabled: boolean) => {
    introSkipperEnabledValue = enabled;
    form.setFieldValue('introSkipperEnabled', enabled);
  };

  const state = () => connectionState();

  return (
    <div class="min-h-screen bg-background p-6 md:p-10">
      <div class="max-w-3xl mx-auto space-y-6">
        {/* Header */}
        <PageHeader
          title="Settings"
          description="Manage your connection and preferences"
          trailing={
            <button
              type="button"
              onClick={handleRefresh}
              class="btn-icon hover:rotate-180 transition-transform"
              title="Refresh status"
            >
              <RefreshCw class="w-6 h-6" />
            </button>
          }
        />

        {/* Jellyfin Connection Card */}
        <SectionCard
          icon={<CircleCheckBig class="w-6 h-6" />}
          title="Jellyfin Connection"
        >
          <Show
            when={!connectionState.loading}
            fallback={
              <div class="animate-pulse h-24 bg-surface-container-high rounded-xl" />
            }
          >
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <InfoCard label="Status">
                <StatusIndicator connected={state()?.connected ?? false} />
              </InfoCard>

              <Show when={state()?.serverName}>
                <InfoCard label="Server">
                  <span
                    class="text-on-surface font-medium truncate block"
                    title={state()?.serverName ?? ''}
                  >
                    {state()?.serverName}
                  </span>
                </InfoCard>
              </Show>

              <Show when={state()?.serverUrl}>
                <InfoCard label="URL">
                  <span
                    class="text-on-surface font-medium truncate block"
                    title={state()?.serverUrl ?? ''}
                  >
                    {state()?.serverUrl}
                  </span>
                </InfoCard>
              </Show>

              <Show when={state()?.userName}>
                <InfoCard label="User">
                  <span
                    class="text-on-surface font-medium truncate block"
                    title={state()?.userName ?? ''}
                  >
                    {state()?.userName}
                  </span>
                </InfoCard>
              </Show>
            </div>
          </Show>
        </SectionCard>

        {/* Settings Form */}
        <form
          onSubmit={(e) => {
            e.preventDefault();
            e.stopPropagation();
            void form.handleSubmit();
          }}
          class="space-y-6"
        >
          {/* Device Settings Card */}
          <SectionCard icon={<Cast class="w-6 h-6" />} title="Device Settings">
            <div class="space-y-4">
              <form.Field
                name="deviceName"
                validators={{
                  onChange: ({ value }) =>
                    !value.trim() ? 'Device name is required' : undefined,
                }}
              >
                {(field) => (
                  <div class="group">
                    <label
                      for={field().name}
                      class="block text-label-small text-on-surface-variant mb-1.5 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                    >
                      Device Name
                    </label>
                    <input
                      id={field().name}
                      name={field().name}
                      type="text"
                      value={field().state.value}
                      onInput={(e) =>
                        field().handleChange(e.currentTarget.value)
                      }
                      onBlur={() => field().handleBlur()}
                      class="input-filled"
                      placeholder="JMSR"
                    />
                    <Show when={field().state.meta.errors.length > 0}>
                      <p class="text-error text-body-small mt-1.5 ml-1">
                        {field().state.meta.errors[0]}
                      </p>
                    </Show>
                    <p class="text-on-surface-variant/70 text-body-small mt-1.5 ml-1">
                      Name displayed in Jellyfin cast menu
                    </p>
                  </div>
                )}
              </form.Field>
            </div>
          </SectionCard>

          {/* MPV Player Card */}
          <SectionCard
            icon={<Play class="w-6 h-6" />}
            title="MPV Player"
            trailing={
              <Show when={!mpvConnected.loading}>
                <StatusBadge variant={mpvConnected() ? 'success' : 'neutral'}>
                  {mpvConnected() ? 'Running' : 'Not Started'}
                </StatusBadge>
              </Show>
            }
          >
            <div class="space-y-6">
              <form.Field name="mpvPath">
                {(field) => (
                  <div class="group">
                    <label
                      for={field().name}
                      class="block text-label-small text-on-surface-variant mb-1.5 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                    >
                      MPV Executable Path
                    </label>
                    <div class="flex gap-2 items-start">
                      <input
                        id={field().name}
                        name={field().name}
                        type="text"
                        value={field().state.value}
                        onInput={(e) =>
                          field().handleChange(e.currentTarget.value)
                        }
                        onBlur={() => field().handleBlur()}
                        placeholder="Path to mpv.exe or mpv binary"
                        class="input-filled flex-1 min-w-0"
                      />
                      <button
                        type="button"
                        onClick={handleDetectMpv}
                        disabled={detectingMpv()}
                        class="btn-tonal h-14"
                      >
                        {detectingMpv() ? '...' : 'Auto-detect'}
                      </button>
                    </div>
                  </div>
                )}
              </form.Field>

              <form.Field name="mpvArgs">
                {(field) => (
                  <div class="group">
                    <label
                      for={field().name}
                      class="block text-label-small text-on-surface-variant mb-1.5 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                    >
                      Extra Arguments (one per line)
                    </label>
                    <textarea
                      id={field().name}
                      name={field().name}
                      value={field().state.value}
                      onInput={(e) =>
                        field().handleChange(e.currentTarget.value)
                      }
                      onBlur={() => field().handleBlur()}
                      rows={4}
                      placeholder="--fullscreen&#10;--force-window"
                      class="w-full bg-surface-container-highest rounded-t-lg border-b border-on-surface-variant px-4 py-3 text-on-surface placeholder-on-surface-variant focus:border-b-2 focus:border-primary focus:outline-none transition-colors font-mono text-body-small leading-relaxed"
                    />
                  </div>
                )}
              </form.Field>
            </div>
          </SectionCard>

          {/* Playback Settings Card */}
          <SectionCard icon={<SkipForward class="w-6 h-6" />} title="Playback">
            <label
              for="intro-skipper-enabled"
              class="flex items-center justify-between gap-4 rounded-xl bg-surface-container-highest px-4 py-3 cursor-pointer"
            >
              <span class="text-body-large text-on-surface">Intro Skipper</span>
              <input
                id="intro-skipper-enabled"
                name="introSkipperEnabled"
                type="checkbox"
                ref={(el) => {
                  introSkipperInput = el;
                }}
                checked={introSkipperEnabledValue}
                onInput={(e) =>
                  handleIntroSkipperToggle(e.currentTarget.checked)
                }
                onChange={(e) =>
                  handleIntroSkipperToggle(e.currentTarget.checked)
                }
                class="h-6 w-6 rounded border-outline text-primary focus:ring-primary"
              />
            </label>
          </SectionCard>

          {/* Keybindings Card */}
          <SectionCard icon={<Keyboard class="w-6 h-6" />} title="Keybindings">
            <p class="text-on-surface-variant/80 text-body-medium mb-6 -mt-4 ml-9">
              Keyboard shortcuts for MPV episode navigation. Changes take effect
              on next MPV restart.
            </p>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
              <form.Field
                name="keybindNext"
                validators={{
                  onChange: ({ value }) =>
                    !value.trim() ? 'Keybinding is required' : undefined,
                }}
              >
                {(field) => (
                  <div class="group">
                    <label
                      for={field().name}
                      class="block text-label-small text-on-surface-variant mb-1.5 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                    >
                      Next Episode
                    </label>
                    <input
                      id={field().name}
                      name={field().name}
                      type="text"
                      value={field().state.value}
                      onInput={(e) =>
                        field().handleChange(e.currentTarget.value)
                      }
                      onBlur={() => field().handleBlur()}
                      class="input-filled font-mono text-center"
                      placeholder="Shift+n"
                    />
                    <Show when={field().state.meta.errors.length > 0}>
                      <p class="text-error text-body-small mt-1.5 ml-1">
                        {field().state.meta.errors[0]}
                      </p>
                    </Show>
                  </div>
                )}
              </form.Field>

              <form.Field
                name="keybindPrev"
                validators={{
                  onChange: ({ value }) =>
                    !value.trim() ? 'Keybinding is required' : undefined,
                }}
              >
                {(field) => (
                  <div class="group">
                    <label
                      for={field().name}
                      class="block text-label-small text-on-surface-variant mb-1.5 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                    >
                      Previous Episode
                    </label>
                    <input
                      id={field().name}
                      name={field().name}
                      type="text"
                      value={field().state.value}
                      onInput={(e) =>
                        field().handleChange(e.currentTarget.value)
                      }
                      onBlur={() => field().handleBlur()}
                      class="input-filled font-mono text-center"
                      placeholder="Shift+p"
                    />
                    <Show when={field().state.meta.errors.length > 0}>
                      <p class="text-error text-body-small mt-1.5 ml-1">
                        {field().state.meta.errors[0]}
                      </p>
                    </Show>
                  </div>
                )}
              </form.Field>
            </div>

            <p class="text-on-surface-variant/60 text-body-small mt-6 text-center border-t border-outline-variant/20 pt-4">
              Use MPV keybinding syntax (e.g., Shift+n, Ctrl+Left, Alt+q)
            </p>
          </SectionCard>

          {/* Save Settings Button */}
          <div class="sticky bottom-6 z-20">
            <form.Subscribe selector={(state) => state.isSubmitting}>
              {(isSubmitting) => (
                <button
                  type="submit"
                  disabled={isSubmitting()}
                  class="btn-primary w-full h-14 text-title-medium shadow-lg hover:shadow-xl hover:-translate-y-1 active:translate-y-0 active:scale-[0.99] backdrop-blur-md"
                >
                  {isSubmitting() ? 'Saving...' : 'Save Settings'}
                </button>
              )}
            </form.Subscribe>

            <Show when={saveMessage()}>
              <div
                class={`mt-4 p-4 rounded-xl text-body-medium font-medium text-center animate-in slide-in-from-bottom-2 fade-in duration-300 ${
                  saveMessage()?.type === 'success'
                    ? 'bg-tertiary-container text-on-tertiary-container'
                    : 'bg-error-container text-on-error-container'
                }`}
              >
                {saveMessage()?.text}
              </div>
            </Show>
          </div>
        </form>

        {/* Actions Card */}
        <div class="card-filled relative overflow-hidden">
          <div class="absolute inset-0 bg-surface-tint/[0.03] pointer-events-none" />
          <div class="relative z-10">
            <h2 class="text-title-medium text-on-surface mb-6">Danger Zone</h2>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <button
                type="button"
                onClick={handleDisconnect}
                disabled={disconnecting() || !state()?.connected}
                class="btn-outlined border-error/50 text-error hover:bg-error/10 hover:border-error w-full"
              >
                <LogOut class="w-5 h-5" />
                {disconnecting() ? 'Disconnecting...' : 'Disconnect'}
              </button>

              <button
                type="button"
                onClick={handleClearSession}
                disabled={clearingSession()}
                class="btn-tonal w-full"
              >
                <Trash2 class="w-5 h-5" />
                {clearingSession() ? 'Clearing...' : 'Clear Session'}
              </button>
            </div>
            <p class="text-on-surface-variant/60 text-body-small mt-4 text-center">
              Clear saved session will remove stored credentials and return to
              login
            </p>
          </div>
        </div>

        <SectionCard
          icon={<ClipboardList class="w-5 h-5" />}
          title="Troubleshooting"
          trailing={
            <button
              type="button"
              class="btn-text min-w-0 px-3"
              onClick={() =>
                setTroubleshootingExpanded((expanded) => !expanded)
              }
              aria-expanded={troubleshootingExpanded()}
              aria-label="Toggle troubleshooting"
            >
              <ChevronDown
                class={`w-5 h-5 text-on-surface-variant transition-transform ${troubleshootingExpanded() ? 'rotate-180' : ''}`}
              />
            </button>
          }
        >
          <div hidden={!troubleshootingExpanded()}>
            <DiagnosticsPanel />
          </div>
        </SectionCard>

        {/* Version Footer */}
        <PageFooter />
      </div>
    </div>
  );
}
