import { createForm } from '@tanstack/solid-form';
import { Check, CircleAlert, LoaderCircle } from 'lucide-solid';
import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { type Credentials, commands } from '../bindings';
import { saveSession } from '../router';
import { Card, PageFooter } from './ui';

interface LoginPageProps {
  onConnected: () => void;
}

const STORAGE_KEY = 'jmsr_saved_credentials';

interface SavedCredentials {
  serverUrl: string;
  username: string;
  rememberMe: boolean;
}

function loadSavedCredentials(): SavedCredentials | null {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      return JSON.parse(saved) as SavedCredentials;
    }
  } catch {
    // Ignore parse errors
  }
  return null;
}

function saveCredentials(serverUrl: string, username: string): void {
  const data: SavedCredentials = { serverUrl, username, rememberMe: true };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
}

function clearSavedCredentials(): void {
  localStorage.removeItem(STORAGE_KEY);
}

type LoginMethod = 'quickConnect' | 'password';
type QuickConnectState = 'idle' | 'waiting' | 'failed';

export default function LoginPage(props: LoginPageProps) {
  const [error, setError] = createSignal<string | null>(null);
  const [loginMethod, setLoginMethod] =
    createSignal<LoginMethod>('quickConnect');
  const [quickConnectState, setQuickConnectState] =
    createSignal<QuickConnectState>('idle');
  const [quickConnectCode, setQuickConnectCode] = createSignal<string | null>(
    null,
  );
  const [isQuickConnectSubmitting, setIsQuickConnectSubmitting] =
    createSignal(false);
  let pollInterval: ReturnType<typeof setInterval> | undefined;
  let timeoutHandle: ReturnType<typeof setTimeout> | undefined;

  const stopQuickConnectPolling = () => {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = undefined;
    }
    if (timeoutHandle) {
      clearTimeout(timeoutHandle);
      timeoutHandle = undefined;
    }
  };

  const resetQuickConnect = () => {
    stopQuickConnectPolling();
    setQuickConnectState('idle');
    setQuickConnectCode(null);
    setError(null);
    setIsQuickConnectSubmitting(false);
  };

  const finishConnected = async () => {
    const session = await commands.jellyfinGetSession();
    if (session) {
      saveSession(session);
    }

    props.onConnected();
  };

  const form = createForm(() => ({
    defaultValues: {
      serverUrl: '',
      username: '',
      password: '',
      rememberMe: false,
    },
    onSubmit: async ({ value }) => {
      setError(null);

      if (loginMethod() === 'quickConnect') {
        await startQuickConnect(value.serverUrl);
        return;
      }

      const credentials: Credentials = {
        serverUrl: value.serverUrl,
        username: value.username,
        password: value.password,
      };

      try {
        const result = await commands.jellyfinConnect(credentials);
        if (result.status === 'ok') {
          // Save or clear credentials based on rememberMe
          if (value.rememberMe) {
            saveCredentials(value.serverUrl, value.username);
          } else {
            clearSavedCredentials();
          }

          await finishConnected();
        } else {
          setError(result.error.message);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Connection failed');
      }
    },
  }));

  const isSubmitting = form.useStore((state) => state.isSubmitting);
  const isQuickConnectWaiting = () => quickConnectState() === 'waiting';

  const checkQuickConnectApproval = async (
    serverUrlValue: string,
    secret: string,
  ) => {
    const result = await commands.jellyfinQuickConnectCheck(
      serverUrlValue,
      secret,
    );
    if (result.status === 'error') {
      stopQuickConnectPolling();
      setQuickConnectState('failed');
      setError(result.error.message);
      return;
    }

    if (result.data !== 'approved') {
      return;
    }

    stopQuickConnectPolling();
    setIsQuickConnectSubmitting(true);
    const authResult = await commands.jellyfinQuickConnectAuthenticate(
      serverUrlValue,
      secret,
    );
    setIsQuickConnectSubmitting(false);

    if (authResult.status === 'error') {
      setQuickConnectState('failed');
      setError(authResult.error.message);
      return;
    }

    await finishConnected();
  };

  const startQuickConnect = async (serverUrlValue: string) => {
    setError(null);
    setIsQuickConnectSubmitting(true);

    try {
      new URL(serverUrlValue);
    } catch {
      setError('Please enter a valid URL');
      setIsQuickConnectSubmitting(false);
      return;
    }

    const result = await commands.jellyfinQuickConnectStart(serverUrlValue);
    setIsQuickConnectSubmitting(false);

    if (result.status === 'error') {
      setQuickConnectState('failed');
      setError(result.error.message);
      return;
    }

    const request = result.data;
    setQuickConnectCode(request.code);
    setQuickConnectState('waiting');

    pollInterval = setInterval(() => {
      void checkQuickConnectApproval(serverUrlValue, request.secret);
    }, 5000);
    timeoutHandle = setTimeout(
      () => {
        stopQuickConnectPolling();
        setQuickConnectState('failed');
        setError(
          'Quick Connect code expired. Request a new code to try again.',
        );
      },
      5 * 60 * 1000,
    );
  };

  onCleanup(stopQuickConnectPolling);

  // Load saved credentials on mount
  onMount(() => {
    const saved = loadSavedCredentials();
    if (saved) {
      form.reset({
        serverUrl: saved.serverUrl,
        username: saved.username,
        password: '',
        rememberMe: saved.rememberMe,
      });
    }
  });

  return (
    <div class="min-h-screen bg-background flex items-center justify-center p-6">
      <div class="w-full max-w-md">
        {/* Logo */}
        <div class="text-center mb-10">
          <h1 class="text-display-medium font-normal text-primary tracking-tight drop-shadow-sm">
            JMSR
          </h1>
          <p class="text-body-large text-on-surface-variant mt-2 tracking-wide font-normal">
            Jellyfin MPV Shim
          </p>
        </div>

        {/* Login Card */}
        <Card variant="elevated" class="relative overflow-hidden">
          {/* Surface Tint Overlay for Elevation 1 */}
          <div class="absolute inset-0 bg-surface-tint/[0.05] pointer-events-none" />

          <div class="relative z-10">
            <h2 class="text-headline-small text-on-surface mb-8 tracking-tight">
              Connect to Server
            </h2>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                e.stopPropagation();
                form.handleSubmit();
              }}
              class="space-y-6"
            >
              {/* Server URL */}
              <form.Field
                name="serverUrl"
                validators={{
                  onBlur: ({ value }) => {
                    if (!value) return 'Server URL is required';
                    try {
                      new URL(value);
                      return undefined;
                    } catch {
                      return 'Please enter a valid URL';
                    }
                  },
                }}
                children={(field) => {
                  const errors = () => field().state.meta.errors;
                  const touched = () => field().state.meta.isTouched;
                  return (
                    <div class="group">
                      <label
                        for={field().name}
                        class="text-label-medium block text-on-surface-variant mb-1 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                      >
                        Server URL
                      </label>
                      <input
                        id={field().name}
                        name={field().name}
                        type="url"
                        value={field().state.value}
                        onInput={(e) =>
                          field().handleChange(e.currentTarget.value)
                        }
                        onBlur={field().handleBlur}
                        placeholder="https://jellyfin.example.com"
                        disabled={isSubmitting() || isQuickConnectWaiting()}
                        class="input-filled w-full"
                      />
                      <Show when={touched() && errors().length > 0}>
                        <p class="text-error text-body-small mt-1.5 ml-1 animate-in slide-in-from-top-1 fade-in duration-200">
                          {errors()[0]}
                        </p>
                      </Show>
                    </div>
                  );
                }}
              />

              <Show
                when={loginMethod() === 'quickConnect'}
                fallback={
                  <>
                    {/* Username */}
                    <form.Field
                      name="username"
                      validators={{
                        onBlur: ({ value }) => {
                          if (!value) return 'Username is required';
                          return undefined;
                        },
                      }}
                      children={(field) => {
                        const errors = () => field().state.meta.errors;
                        const touched = () => field().state.meta.isTouched;
                        return (
                          <div class="group">
                            <label
                              for={field().name}
                              class="text-label-medium block text-on-surface-variant mb-1 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                            >
                              Username
                            </label>
                            <input
                              id={field().name}
                              name={field().name}
                              type="text"
                              value={field().state.value}
                              onInput={(e) =>
                                field().handleChange(e.currentTarget.value)
                              }
                              onBlur={field().handleBlur}
                              placeholder="Enter your username"
                              disabled={isSubmitting()}
                              class="input-filled w-full"
                            />
                            <Show when={touched() && errors().length > 0}>
                              <p class="text-error text-body-small mt-1.5 ml-1 animate-in slide-in-from-top-1 fade-in duration-200">
                                {errors()[0]}
                              </p>
                            </Show>
                          </div>
                        );
                      }}
                    />

                    {/* Password */}
                    <form.Field
                      name="password"
                      children={(field) => (
                        <div class="group">
                          <label
                            for={field().name}
                            class="text-label-medium block text-on-surface-variant mb-1 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
                          >
                            Password
                          </label>
                          <input
                            id={field().name}
                            name={field().name}
                            type="password"
                            value={field().state.value}
                            onInput={(e) =>
                              field().handleChange(e.currentTarget.value)
                            }
                            onBlur={field().handleBlur}
                            placeholder="Enter your password"
                            disabled={isSubmitting()}
                            class="input-filled w-full"
                          />
                        </div>
                      )}
                    />

                    {/* Remember Me */}
                    <form.Field
                      name="rememberMe"
                      children={(field) => (
                        <div class="flex items-center gap-3 py-1">
                          <div class="relative flex items-center">
                            <input
                              id={field().name}
                              name={field().name}
                              type="checkbox"
                              checked={field().state.value}
                              onChange={(e) =>
                                field().handleChange(e.currentTarget.checked)
                              }
                              disabled={isSubmitting()}
                              class="peer w-5 h-5 rounded border-2 border-on-surface-variant checked:border-primary bg-transparent checked:bg-primary text-primary focus:ring-primary focus:ring-2 focus:ring-offset-0 focus:ring-offset-surface transition-all duration-200 cursor-pointer appearance-none"
                            />
                            <Check
                              class="absolute pointer-events-none opacity-0 peer-checked:opacity-100 text-on-primary w-3.5 h-3.5 left-1 top-1 transition-opacity duration-200"
                              stroke-width={4}
                            />
                          </div>
                          <label
                            for={field().name}
                            class="text-body-medium text-on-surface select-none cursor-pointer"
                          >
                            Remember server and username
                          </label>
                        </div>
                      )}
                    />
                  </>
                }
              >
                <Show when={quickConnectCode()}>
                  <div class="rounded-lg bg-surface-container-highest border border-outline-variant p-4 text-center">
                    <p class="text-label-medium text-on-surface-variant uppercase">
                      Quick Connect Code
                    </p>
                    <p class="text-display-small text-primary font-mono tracking-normal mt-2">
                      {quickConnectCode()}
                    </p>
                    <p class="text-body-medium text-on-surface-variant mt-2">
                      Approve this code from a signed-in Jellyfin app.
                    </p>
                  </div>
                </Show>

                <Show when={isQuickConnectWaiting()}>
                  <p class="text-body-medium text-on-surface-variant">
                    Waiting for Quick Connect approval...
                  </p>
                </Show>
              </Show>

              {/* Error Message */}
              <Show when={error()}>
                <div class="p-4 bg-error-container text-on-error-container rounded-xl animate-in slide-in-from-top-2 fade-in duration-300 flex items-start gap-3">
                  <div class="mt-0.5">
                    <CircleAlert class="w-5 h-5" />
                  </div>
                  <p class="text-body-medium font-medium">{error()}</p>
                </div>
              </Show>

              {/* Submit Button */}
              <Show
                when={isQuickConnectWaiting()}
                fallback={
                  <button
                    type="submit"
                    disabled={isSubmitting() || isQuickConnectSubmitting()}
                    class="btn-primary w-full"
                  >
                    <Show
                      when={isSubmitting() || isQuickConnectSubmitting()}
                      fallback={
                        loginMethod() === 'quickConnect'
                          ? quickConnectState() === 'failed'
                            ? 'Try again'
                            : 'Request code'
                          : 'Connect'
                      }
                    >
                      <LoaderCircle class="animate-spin h-5 w-5" />
                      <span>
                        {loginMethod() === 'quickConnect'
                          ? 'Requesting...'
                          : 'Connecting...'}
                      </span>
                    </Show>
                  </button>
                }
              >
                <button
                  type="button"
                  class="btn-secondary w-full"
                  onClick={resetQuickConnect}
                >
                  Cancel
                </button>
              </Show>

              <button
                type="button"
                class="btn-text w-full"
                disabled={isQuickConnectWaiting()}
                onClick={() => {
                  resetQuickConnect();
                  setLoginMethod(
                    loginMethod() === 'quickConnect'
                      ? 'password'
                      : 'quickConnect',
                  );
                }}
              >
                {loginMethod() === 'quickConnect'
                  ? 'Use password instead'
                  : 'Use Quick Connect'}
              </button>
            </form>
          </div>
        </Card>

        {/* Footer */}
        <PageFooter class="mt-8" />
      </div>
    </div>
  );
}
