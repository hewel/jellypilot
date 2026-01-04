import { createForm } from '@tanstack/solid-form';
import {
  createFileRoute,
  Outlet,
  redirect,
  useLocation,
  useNavigate,
} from '@tanstack/solid-router';
import {
  Cast,
  CircleCheckBig,
  FileText,
  Keyboard,
  Play,
  RefreshCw,
  ShieldAlert,
} from 'lucide-solid';
import { createEffect, createResource, createSignal, Show } from 'solid-js';
import { css, cx } from '../../styled-system/css';
import { button } from '../../styled-system/recipes';
import { type AppConfig, type ConnectionState, commands } from '../bindings';
import { useToast } from '../components/ToastProvider';
import { PageFooter, PageHeader } from '../components/ui';
import Sidebar, { type SidebarTab } from '../components/ui/Sidebar';
import { checkAuth, clearSavedSession } from '../lib/auth';
import {
  SettingsContext,
  type SettingsContextValue,
} from './settings/_context';

export const Route = createFileRoute('/settings')({
  beforeLoad: async () => {
    const isAuth = await checkAuth();
    if (!isAuth) {
      throw redirect({ to: '/login' });
    }
  },
  component: SettingsLayout,
});

async function fetchConnectionState(): Promise<ConnectionState> {
  return await commands.jellyfinGetState();
}

async function fetchMpvStatus(): Promise<boolean> {
  return await commands.mpvIsConnected();
}

function SettingsLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const { showToast } = useToast();

  const [disconnecting, setDisconnecting] = createSignal(false);
  const [clearingSession, setClearingSession] = createSignal(false);
  const [detectingMpv, setDetectingMpv] = createSignal(false);
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

  const form = createForm(() => ({
    defaultValues: {
      deviceName: 'JMSR',
      mpvPath: '',
      mpvArgs: '',
      keybindNext: 'Shift+n',
      keybindPrev: 'Shift+p',
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

  createEffect(() => {
    const cfg = initialConfig();
    if (cfg) {
      form.setFieldValue('deviceName', cfg.deviceName ?? 'JMSR');
      form.setFieldValue('mpvPath', cfg.mpvPath ?? '');
      form.setFieldValue('mpvArgs', (cfg.mpvArgs ?? []).join('\n'));
      form.setFieldValue('keybindNext', cfg.keybindNext ?? 'Shift+n');
      form.setFieldValue('keybindPrev', cfg.keybindPrev ?? 'Shift+p');
    }
  });

  const handleDisconnect = async () => {
    setDisconnecting(true);
    try {
      const result = await commands.jellyfinDisconnect();
      if (result.status === 'ok') {
        clearSavedSession();
        navigate({ to: '/login' });
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
      navigate({ to: '/login' });
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

  const tabs: SidebarTab[] = [
    { id: 'connection', label: 'Connection', icon: CircleCheckBig },
    { id: 'device', label: 'Device', icon: Cast },
    { id: 'player', label: 'Player', icon: Play },
    { id: 'keybindings', label: 'Keys', icon: Keyboard },
    { id: 'actions', label: 'Actions', icon: ShieldAlert },
    { id: 'logs', label: 'Logs', icon: FileText },
  ];

  const activeTab = () => {
    const path = location().pathname;
    if (path.includes('/settings/device')) return 'device';
    if (path.includes('/settings/player')) return 'player';
    if (path.includes('/settings/keybindings')) return 'keybindings';
    if (path.includes('/settings/actions')) return 'actions';
    if (path.includes('/settings/logs')) return 'logs';
    return 'connection';
  };

  const contextValue: SettingsContextValue = {
    form,
    connectionState,
    mpvConnected,
    detectingMpv,
    setDetectingMpv,
    saveMessage,
    disconnecting,
    clearingSession,
    handleDisconnect,
    handleClearSession,
    handleDetectMpv,
  };

  return (
    <SettingsContext.Provider value={contextValue}>
      <Sidebar
        tabs={tabs}
        value={activeTab()}
        onValueChange={(value) => {
          if (value === 'connection') navigate({ to: '/settings' });
          else navigate({ to: `/settings/${value}` });
        }}
      >
        <div
          class={css({
            maxWidth: '800px',
            width: '100%',
            marginX: 'auto',
            padding: '24px',
            md: { padding: '40px' },
            display: 'flex',
            flexDirection: 'column',
            gap: '24px',
            paddingBottom: '80px',
          })}
        >
          <PageHeader
            title="Settings"
            description={`${tabs.find((t) => t.id === activeTab())?.label} Settings`}
            trailing={
              <button
                type="button"
                onClick={handleRefresh}
                class={cx(
                  button({ variant: 'icon' }),
                  css({
                    _hover: { transform: 'rotate(180deg)' },
                    transition: 'transform 0.3s',
                  }),
                )}
                title="Refresh status"
              >
                <RefreshCw class={css({ width: '24px', height: '24px' })} />
              </button>
            }
          />

          <Outlet />

          {/* Sticky Save Button - only show for form tabs */}
          <Show
            when={['device', 'player', 'keybindings'].includes(activeTab())}
          >
            <div
              class={css({
                position: 'sticky',
                bottom: '24px',
                zIndex: 20,
              })}
            >
              <form.Subscribe selector={(state) => state.isSubmitting}>
                {(isSubmitting) => (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.preventDefault();
                      form.handleSubmit();
                    }}
                    disabled={isSubmitting()}
                    class={cx(
                      button({ variant: 'primary' }),
                      css({
                        width: '100%',
                        height: '56px',
                        textStyle: 'titleMedium',
                        boxShadow: 'lg',
                        _hover: {
                          boxShadow: 'xl',
                          transform: 'translateY(-4px)',
                        },
                        _active: {
                          transform: 'translateY(0) scale(0.99)',
                        },
                        backdropFilter: 'blur(12px)',
                      }),
                    )}
                  >
                    {isSubmitting() ? 'Saving...' : 'Save Settings'}
                  </button>
                )}
              </form.Subscribe>

              <Show when={saveMessage()}>
                <div
                  class={cx(
                    css({
                      marginTop: '16px',
                      padding: '16px',
                      borderRadius: '12px',
                      textStyle: 'bodyMedium',
                      fontWeight: 'medium',
                      textAlign: 'center',
                      animation:
                        'slideInFromBottom 0.3s ease-out, fadeIn 0.3s ease-out',
                    }),
                    saveMessage()?.type === 'success'
                      ? css({
                          backgroundColor: 'tertiaryContainer',
                          color: 'onTertiaryContainer',
                        })
                      : css({
                          backgroundColor: 'errorContainer',
                          color: 'onErrorContainer',
                        }),
                  )}
                >
                  {saveMessage()?.text}
                </div>
              </Show>
            </div>
          </Show>

          <PageFooter />
        </div>
      </Sidebar>
    </SettingsContext.Provider>
  );
}
