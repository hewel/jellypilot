import { Checkbox } from '@ark-ui/solid/checkbox';
import { Field as ArkField } from '@ark-ui/solid/field';
import { Tabs } from '@ark-ui/solid/tabs';
import { createForm } from '@tanstack/solid-form';
import { Effect, Exit } from 'effect';
import { Check, CircleAlert, LoaderCircle, RadioTower } from 'lucide-solid';
import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { type Credentials, commands } from '../bindings';
import { buildServerUrlEffect } from '../effects/serverUrl';
import {
  clearSavedCredentials,
  loadSavedCredentials,
  saveCredentials,
} from '../effects/session';
import {
  defaultSchemeForHost,
  explicitSchemeFromInput,
  parseServerUrl,
  type ServerScheme,
  type ServerUrlResult,
  stripServerScheme,
} from '../serverUrl';
import { saveCurrentSession } from '../sessionAccess';
import { Card, PageFooter } from './ui';

interface LoginPageProps {
  onConnected: () => void;
}
type LoginMethod = 'quickConnect' | 'password';
type QuickConnectState = 'idle' | 'waiting' | 'failed';

interface LoginValues {
  scheme: ServerScheme;
  host: string;
  username: string;
  password: string;
  rememberMe: boolean;
}

type ServerUrlValidation =
  | { status: 'ok'; result: ServerUrlResult }
  | { status: 'error'; message: string };

export default function LoginPage(props: LoginPageProps) {
  const [loginMethod, setLoginMethod] =
    createSignal<LoginMethod>('quickConnect');
  const [quickConnectState, setQuickConnectState] =
    createSignal<QuickConnectState>('idle');
  const [quickConnectCode, setQuickConnectCode] = createSignal<string | null>(
    null,
  );
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  let pollInterval: ReturnType<typeof setInterval> | undefined;
  let timeoutHandle: ReturnType<typeof setTimeout> | undefined;

  const form = createForm(() => ({
    defaultValues: {
      scheme: 'https' as ServerScheme,
      host: '',
      username: '',
      password: '',
      rememberMe: false,
    },
    onSubmit: async ({ value }) => {
      if (loginMethod() === 'quickConnect') {
        await startQuickConnect(value);
      } else {
        await connectWithPassword(value);
      }
    },
  }));
  const formValues = form.useStore((state) => state.values);

  const isQuickConnectWaiting = () => quickConnectState() === 'waiting';

  const validateServerUrl = (
    value: Pick<LoginValues, 'scheme' | 'host'>,
  ): ServerUrlValidation =>
    Effect.runSync(
      buildServerUrlEffect({
        scheme: value.scheme,
        host: value.host,
      }).pipe(
        Effect.match({
          onFailure: (err) => ({ status: 'error', message: err.message }),
          onSuccess: (result) => ({ status: 'ok', result }),
        }),
      ),
    );

  const serverUrlResult = () => {
    const validation = validateServerUrl({
      scheme: formValues().scheme,
      host: formValues().host,
    });
    return validation.status === 'ok' ? validation.result : null;
  };

  const serverUrl = () => serverUrlResult()?.url ?? '';

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
    setSubmitting(false);
  };

  const finishConnected = async () => {
    await saveCurrentSession();
    props.onConnected();
  };

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

    if (result.data !== 'approved') return;

    stopQuickConnectPolling();
    setSubmitting(true);
    const authResult = await commands.jellyfinQuickConnectAuthenticate(
      serverUrlValue,
      secret,
    );
    setSubmitting(false);

    if (authResult.status === 'error') {
      setQuickConnectState('failed');
      setError(authResult.error.message);
      return;
    }

    await finishConnected();
  };

  const startQuickConnect = async (value: LoginValues) => {
    setError(null);
    const validation = validateServerUrl(value);
    if (validation.status === 'error') {
      setError(validation.message);
      return;
    }

    setSubmitting(true);
    const serverUrlValue = validation.result.url;
    const result = await commands.jellyfinQuickConnectStart(serverUrlValue);
    setSubmitting(false);

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

  const connectWithPassword = async (value: LoginValues) => {
    setError(null);
    const validation = validateServerUrl(value);
    if (validation.status === 'error') {
      setError(validation.message);
      return;
    }
    if (!value.username.trim()) {
      setError('Username is required');
      return;
    }

    const finalServerUrl = validation.result.url;
    const credentials: Credentials = {
      serverUrl: finalServerUrl,
      username: value.username,
      password: value.password,
    };

    setSubmitting(true);
    try {
      const result = await commands.jellyfinConnect(credentials);
      if (result.status === 'ok') {
        if (value.rememberMe)
          Effect.runSync(saveCredentials(finalServerUrl, value.username));
        else Effect.runSync(clearSavedCredentials());
        await finishConnected();
      } else {
        setError(result.error.message);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Connection failed');
    } finally {
      setSubmitting(false);
    }
  };

  const submit = () => {
    void form.handleSubmit();
  };

  onMount(() => {
    const exit = Effect.runSyncExit(loadSavedCredentials());
    if (!Exit.isSuccess(exit)) return;
    const saved = exit.value;
    if (!saved) return;
    const parsed = parseServerUrl(saved.serverUrl);
    form.reset({
      scheme: parsed.scheme,
      host: parsed.host,
      username: saved.username,
      password: '',
      rememberMe: saved.rememberMe,
    });
  });

  onCleanup(stopQuickConnectPolling);

  return (
    <div class="console-shell flex items-center justify-center">
      <main class="w-full max-w-5xl">
        <div class="mb-8 text-center">
          <p class="text-label-medium uppercase text-secondary">
            Docking Sequence
          </p>
          <h1 class="brand-type mt-2 text-display-medium text-on-surface">
            JMSR
          </h1>
          <p class="mt-2 text-body-large text-on-surface-variant">
            Connect this Playback Target to a known Jellyfin server.
          </p>
        </div>

        <Card variant="elevated" class="mx-auto max-w-3xl">
          <div class="space-y-7">
            <div>
              <h2 class="text-headline-small text-on-surface">
                Server coordinates
              </h2>
              <p class="mt-1 text-body-medium text-on-surface-variant">
                Choose the protocol and host. JMSR shows the final Server URL
                before any Login Method starts.
              </p>
            </div>

            <div class="grid grid-cols-1 gap-3 sm:grid-cols-[auto_minmax(0,1fr)]">
              <form.Field name="scheme">
                {(field) => (
                  <fieldset
                    class="grid grid-cols-2 rounded-2xl border border-outline-variant bg-surface-container-high p-1"
                    aria-label="Server protocol"
                  >
                    <button
                      type="button"
                      class={`rounded-xl px-4 py-3 text-label-large transition ${field().state.value === 'https' ? 'bg-primary text-on-primary' : 'text-on-surface-variant hover:bg-primary/10'}`}
                      disabled={isQuickConnectWaiting()}
                      onClick={() => field().handleChange('https')}
                    >
                      HTTPS
                    </button>
                    <button
                      type="button"
                      class={`rounded-xl px-4 py-3 text-label-large transition ${field().state.value === 'http' ? 'bg-primary text-on-primary' : 'text-on-surface-variant hover:bg-primary/10'}`}
                      disabled={isQuickConnectWaiting()}
                      onClick={() => field().handleChange('http')}
                    >
                      HTTP
                    </button>
                  </fieldset>
                )}
              </form.Field>
              <form.Field name="host">
                {(field) => (
                  <ArkField.Root
                    class="block"
                    disabled={isQuickConnectWaiting()}
                  >
                    <ArkField.Label class="sr-only">
                      Jellyfin host
                    </ArkField.Label>
                    <ArkField.Input
                      type="text"
                      value={field().state.value}
                      onInput={(event) => {
                        const value = event.currentTarget.value;
                        const explicitScheme = explicitSchemeFromInput(value);
                        const strippedHost = stripServerScheme(value);
                        field().handleChange(strippedHost);
                        form.setFieldValue(
                          'scheme',
                          explicitScheme ?? defaultSchemeForHost(value),
                        );
                      }}
                      class="input-filled w-full"
                      placeholder="jellyfin.local or media.example.com/jellyfin"
                    />
                  </ArkField.Root>
                )}
              </form.Field>
            </div>

            <div class="rounded-2xl border border-outline-variant bg-surface-container-lowest p-4">
              <p class="text-label-small uppercase text-on-surface-variant">
                Server URL preview
              </p>
              <p
                class={`mt-1 break-all font-mono text-body-medium ${serverUrl() ? 'text-secondary' : 'text-warning'}`}
              >
                {serverUrl() || 'Enter a server host to preview the final URL'}
              </p>
            </div>

            <Tabs.Root
              value={loginMethod()}
              activationMode="manual"
              lazyMount
              unmountOnExit
              onValueChange={(details) => {
                const value = details.value as LoginMethod;
                if (value !== 'quickConnect' && value !== 'password') return;
                resetQuickConnect();
                setLoginMethod(value);
              }}
            >
              <Tabs.List
                class="grid grid-cols-2 rounded-full border border-outline-variant bg-surface-container-high p-1"
                aria-label="Login Method"
              >
                <Tabs.Trigger
                  value="quickConnect"
                  disabled={isQuickConnectWaiting()}
                  class={`rounded-full px-4 py-3 text-label-large transition ${loginMethod() === 'quickConnect' ? 'bg-primary text-on-primary' : 'text-on-surface-variant hover:bg-primary/10'}`}
                >
                  Quick Connect
                </Tabs.Trigger>
                <Tabs.Trigger
                  value="password"
                  disabled={isQuickConnectWaiting()}
                  class={`rounded-full px-4 py-3 text-label-large transition ${loginMethod() === 'password' ? 'bg-primary text-on-primary' : 'text-on-surface-variant hover:bg-primary/10'}`}
                >
                  Password
                </Tabs.Trigger>
              </Tabs.List>

              <Tabs.Content value="quickConnect">
                <div class="rounded-3xl border border-secondary/30 bg-secondary-container/60 p-5 text-center">
                  <RadioTower class="mx-auto h-8 w-8 text-secondary" />
                  <p class="mt-3 text-body-medium text-on-secondary-container">
                    Approve this code from another signed-in Jellyfin client.
                    JMSR will finish login automatically after approval.
                  </p>
                  <p class="mt-2 text-body-small text-on-surface-variant">
                    You are authorizing this Playback Target.
                  </p>
                  <Show when={quickConnectCode()}>
                    <p class="mt-5 font-mono text-display-small tracking-[0.35em] text-secondary">
                      {quickConnectCode()}
                    </p>
                  </Show>
                  <Show when={isQuickConnectWaiting()}>
                    <p class="mt-4 text-label-medium uppercase text-secondary">
                      Awaiting Quick Connect Approval…
                    </p>
                  </Show>
                </div>
              </Tabs.Content>

              <Tabs.Content value="password">
                <div class="space-y-4">
                  <form.Field name="username">
                    {(field) => (
                      <ArkField.Root class="block">
                        <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          Username
                        </ArkField.Label>
                        <ArkField.Input
                          class="input-filled w-full"
                          value={field().state.value}
                          onInput={(event) =>
                            field().handleChange(event.currentTarget.value)
                          }
                          placeholder="Jellyfin username"
                        />
                      </ArkField.Root>
                    )}
                  </form.Field>
                  <form.Field name="password">
                    {(field) => (
                      <ArkField.Root class="block">
                        <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                          Password
                        </ArkField.Label>
                        <ArkField.Input
                          type="password"
                          class="input-filled w-full"
                          value={field().state.value}
                          onInput={(event) =>
                            field().handleChange(event.currentTarget.value)
                          }
                          placeholder="Jellyfin password"
                        />
                      </ArkField.Root>
                    )}
                  </form.Field>
                  <form.Field name="rememberMe">
                    {(field) => (
                      <Checkbox.Root
                        checked={field().state.value}
                        onCheckedChange={(details) =>
                          field().handleChange(details.checked === true)
                        }
                        class="ark-checkbox text-body-medium text-on-surface"
                      >
                        <Checkbox.Control class="ark-checkbox__control">
                          <Checkbox.Indicator class="ark-checkbox__indicator">
                            <Check class="h-3.5 w-3.5" stroke-width={4} />
                          </Checkbox.Indicator>
                        </Checkbox.Control>
                        <Checkbox.Label>
                          Remember Server URL and username
                        </Checkbox.Label>
                        <Checkbox.HiddenInput />
                      </Checkbox.Root>
                    )}
                  </form.Field>
                </div>
              </Tabs.Content>
            </Tabs.Root>

            <Show when={error()}>
              <div
                class="flex items-start gap-3 rounded-2xl bg-error-container p-4 text-on-error-container"
                role="alert"
              >
                <CircleAlert class="mt-0.5 h-5 w-5" />
                <div>
                  <p class="text-title-small">Connection needs attention</p>
                  <p class="text-body-medium">{error()}</p>
                </div>
              </div>
            </Show>

            <Show
              when={isQuickConnectWaiting()}
              fallback={
                <button
                  type="button"
                  disabled={submitting()}
                  class="btn-primary w-full"
                  onClick={submit}
                >
                  <Show
                    when={submitting()}
                    fallback={
                      loginMethod() === 'quickConnect'
                        ? quickConnectState() === 'failed'
                          ? 'Request a new code'
                          : 'Request Quick Connect code'
                        : 'Connect'
                    }
                  >
                    <LoaderCircle class="h-5 w-5 animate-spin" />
                    {loginMethod() === 'quickConnect'
                      ? 'Requesting...'
                      : 'Connecting...'}
                  </Show>
                </button>
              }
            >
              <button
                type="button"
                class="btn-secondary w-full"
                onClick={resetQuickConnect}
              >
                Cancel Request
              </button>
            </Show>
          </div>
        </Card>

        <PageFooter class="mt-8" />
      </main>
    </div>
  );
}
