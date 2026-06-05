import { Checkbox } from '@ark-ui/solid/checkbox';
import { Field as ArkField } from '@ark-ui/solid/field';
import { Tabs } from '@ark-ui/solid/tabs';
import { createForm } from '@tanstack/solid-form';
import { Effect, Exit } from 'effect';
import { Check, CircleAlert, LoaderCircle, RadioTower } from 'lucide-solid';
import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { type Credentials, commands } from '../bindings';
import { commandFailureMessage, runTauriCommand } from '../effects/commands';
import { CommandError } from '../effects/errors';
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
  let quickConnectRequestId = 0;
  let quickConnectPollingRequestId: number | null = null;

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
    quickConnectRequestId += 1;
    quickConnectPollingRequestId = null;
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
    requestId: number,
  ) => {
    if (
      requestId !== quickConnectRequestId ||
      quickConnectPollingRequestId !== null
    )
      return;
    quickConnectPollingRequestId = requestId;
    const check = await Effect.runPromiseExit(
      runTauriCommand(() =>
        commands.jellyfinQuickConnectCheck(serverUrlValue, secret),
      ),
    );
    if (quickConnectPollingRequestId === requestId)
      quickConnectPollingRequestId = null;
    if (requestId !== quickConnectRequestId) return;
    if (Exit.isFailure(check)) {
      quickConnectRequestId += 1;
      stopQuickConnectPolling();
      setQuickConnectState('failed');
      setError(
        commandFailureMessage(check.cause, 'Quick Connect approval failed'),
      );
      return;
    }

    if (quickConnectPollingRequestId === requestId)
      quickConnectPollingRequestId = null;
    if (check.value !== 'approved') return;

    stopQuickConnectPolling();
    quickConnectRequestId += 1;
    const authRequestId = quickConnectRequestId;
    setSubmitting(true);
    const auth = await Effect.runPromiseExit(
      runTauriCommand(() =>
        commands.jellyfinQuickConnectAuthenticate(serverUrlValue, secret),
      ),
    );
    if (authRequestId !== quickConnectRequestId) return;

    if (Exit.isFailure(auth)) {
      quickConnectRequestId += 1;
      setSubmitting(false);
      setQuickConnectState('failed');
      setError(
        commandFailureMessage(
          auth.cause,
          'Quick Connect authentication failed',
        ),
      );
      return;
    }

    const completion = await Effect.runPromiseExit(
      Effect.tryPromise({
        try: finishConnected,
        catch: (error) =>
          new CommandError({
            message:
              error instanceof Error ? error.message : 'Connection failed',
          }),
      }),
    );
    if (authRequestId !== quickConnectRequestId) return;
    if (Exit.isFailure(completion)) {
      quickConnectRequestId += 1;
      setSubmitting(false);
      setQuickConnectState('failed');
      setError(commandFailureMessage(completion.cause, 'Connection failed'));
    }
  };

  const startQuickConnect = async (value: LoginValues) => {
    setError(null);
    const validation = validateServerUrl(value);
    if (validation.status === 'error') {
      setError(validation.message);
      return;
    }

    setSubmitting(true);
    quickConnectRequestId += 1;
    const requestId = quickConnectRequestId;
    const serverUrlValue = validation.result.url;
    const start = await Effect.runPromiseExit(
      runTauriCommand(() => commands.jellyfinQuickConnectStart(serverUrlValue)),
    );
    if (requestId !== quickConnectRequestId) return;
    setSubmitting(false);

    if (Exit.isFailure(start)) {
      setQuickConnectState('failed');
      setError(commandFailureMessage(start.cause, 'Quick Connect failed'));
      return;
    }

    const request = start.value;
    setQuickConnectCode(request.code);
    setQuickConnectState('waiting');

    pollInterval = setInterval(() => {
      void checkQuickConnectApproval(serverUrlValue, request.secret, requestId);
    }, 5000);
    timeoutHandle = setTimeout(
      () => {
        if (requestId !== quickConnectRequestId) return;
        quickConnectRequestId += 1;
        quickConnectPollingRequestId = null;
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
    const exit = await Effect.runPromiseExit(
      runTauriCommand(() => commands.jellyfinConnect(credentials)),
    );

    if (Exit.isSuccess(exit)) {
      const completion = await Effect.runPromiseExit(
        Effect.tryPromise({
          try: async () => {
            if (value.rememberMe)
              Effect.runSync(saveCredentials(finalServerUrl, value.username));
            else Effect.runSync(clearSavedCredentials());
            await finishConnected();
          },
          catch: (error) =>
            new CommandError({
              message:
                error instanceof Error ? error.message : 'Connection failed',
            }),
        }),
      );

      if (Exit.isFailure(completion)) {
        setSubmitting(false);
        setError(commandFailureMessage(completion.cause, 'Connection failed'));
      }
      return;
    }

    setSubmitting(false);
    setError(commandFailureMessage(exit.cause, 'Connection failed'));
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

  onCleanup(() => {
    quickConnectRequestId += 1;
    quickConnectPollingRequestId = null;
    stopQuickConnectPolling();
  });

  return (
    <div class="console-shell flex items-center justify-center py-10 relative overflow-y-auto">
      <main class="w-full max-w-3xl relative z-10">
        <div class="mb-8 text-center relative">
          {/* Glowing HUD hologram decoration */}
          <div class="relative mx-auto mb-6 flex h-24 w-24 items-center justify-center rounded-full border border-primary/20 bg-primary/5 shadow-brand-glow">
            <div class="absolute inset-0 rounded-full border border-primary/30 animate-ping opacity-25" />
            <div class="absolute inset-2 rounded-full border border-secondary/25 animate-pulse" />
            <div class="absolute inset-4 rounded-full border border-dashed border-primary/10 animate-[spin_60s_linear_infinite]" />
            <RadioTower class="h-10 w-10 text-primary drop-shadow-brand-glow" />
          </div>

          <div class="inline-flex items-center gap-2.5 px-3.5 py-1 rounded-full border border-secondary/20 bg-secondary/5 mb-3.5">
            <span class="w-1.5 h-1.5 rounded-full bg-secondary animate-pulse shadow-[0_0_8px_#818cf8]" />
            <p class="text-[10px] font-bold uppercase tracking-[0.18em] text-secondary">
              Docking Sequence
            </p>
          </div>

          <h1 class="brand-type text-display-medium text-on-surface">JMSR</h1>
          <p class="mt-2 text-body-large text-on-surface-variant max-w-md mx-auto">
            Connect this Playback Target to a known Jellyfin server.
          </p>
        </div>

        <Card
          variant="elevated"
          class="mx-auto shadow-2xl relative overflow-hidden"
        >
          <div class="absolute top-0 left-0 w-full h-[2px] bg-gradient-to-r from-transparent via-primary/55 to-transparent" />
          <div class="space-y-7">
            <div>
              <h2 class="text-headline-small text-on-surface flex items-center gap-2.5">
                <span class="w-1.5 h-5 rounded bg-primary" />
                Server coordinates
              </h2>
              <p class="mt-1.5 text-body-medium text-on-surface-variant">
                Choose the protocol and host. JMSR shows the final Server URL
                before any Login Method starts.
              </p>
            </div>

            <div class="grid grid-cols-1 gap-3 sm:grid-cols-[auto_minmax(0,1fr)]">
              <form.Field name="scheme">
                {(field) => (
                  <fieldset
                    class="grid grid-cols-2 rounded-2xl border border-outline-variant bg-surface-container-high/40 p-1"
                    aria-label="Server protocol"
                  >
                    <button
                      type="button"
                      class={`rounded-xl px-4 py-3 text-label-large cursor-pointer transition-all duration-300 ${field().state.value === 'https' ? 'bg-primary bg-brand-gradient text-on-primary shadow-md shadow-primary/20 font-bold' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
                      disabled={isQuickConnectWaiting()}
                      onClick={() => field().handleChange('https')}
                    >
                      HTTPS
                    </button>
                    <button
                      type="button"
                      class={`rounded-xl px-4 py-3 text-label-large cursor-pointer transition-all duration-300 ${field().state.value === 'http' ? 'bg-primary bg-brand-gradient text-on-primary shadow-md shadow-primary/20 font-bold' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
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

            <div class="rounded-2xl border border-outline-variant bg-surface-container-lowest/40 p-4 relative overflow-hidden backdrop-blur-sm">
              <div class="absolute inset-y-0 left-0 w-[3px] bg-secondary" />
              <p class="text-label-small uppercase text-on-surface-variant/90">
                Server URL preview
              </p>
              <p
                class={`mt-1 break-all font-mono text-body-medium ${serverUrl() ? 'text-secondary font-semibold drop-shadow-[0_0_8px_rgba(129,140,248,0.15)]' : 'text-warning'}`}
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
                class="grid grid-cols-2 rounded-2xl border border-outline-variant bg-surface-container-high/40 p-1 mb-6"
                aria-label="Login Method"
              >
                <Tabs.Trigger
                  value="quickConnect"
                  disabled={isQuickConnectWaiting()}
                  class={`rounded-xl px-4 py-3 text-label-large cursor-pointer transition-all duration-300 ${loginMethod() === 'quickConnect' ? 'bg-brand-gradient text-on-primary shadow-lg shadow-primary/25 font-bold' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
                >
                  Quick Connect
                </Tabs.Trigger>
                <Tabs.Trigger
                  value="password"
                  disabled={isQuickConnectWaiting()}
                  class={`rounded-xl px-4 py-3 text-label-large cursor-pointer transition-all duration-300 ${loginMethod() === 'password' ? 'bg-brand-gradient text-on-primary shadow-lg shadow-primary/25 font-bold' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
                >
                  Password
                </Tabs.Trigger>
              </Tabs.List>

              <Tabs.Content value="quickConnect">
                <div class="rounded-3xl border border-secondary/25 bg-secondary-container/20 p-6 text-center backdrop-blur-sm relative overflow-hidden transition-all duration-300">
                  <div class="absolute inset-0 bg-gradient-to-b from-secondary/5 to-transparent pointer-events-none" />

                  {/* Decorative radar background */}
                  <div class="relative mx-auto mb-4 flex h-20 w-20 items-center justify-center rounded-full border border-secondary/20 bg-secondary/5">
                    <Show when={isQuickConnectWaiting()}>
                      <div class="absolute inset-0 rounded-full border border-secondary/40 animate-radar-ring" />
                      <div
                        class="absolute inset-0 rounded-full border border-secondary/30 animate-radar-ring"
                        style="animation-delay: 0.7s"
                      />
                      <div
                        class="absolute inset-0 rounded-full border border-secondary/20 animate-radar-ring"
                        style="animation-delay: 1.4s"
                      />
                    </Show>
                    <RadioTower
                      class={`h-9 w-9 text-secondary ${isQuickConnectWaiting() ? 'animate-pulse' : ''} drop-shadow-[0_0_8px_rgba(129,140,248,0.4)]`}
                    />
                  </div>

                  <p class="text-body-medium text-on-secondary-container font-medium">
                    Approve this code from another signed-in Jellyfin client.
                    JMSR will finish login automatically after approval.
                  </p>
                  <p class="mt-2 text-body-small text-on-surface-variant/80">
                    You are authorizing this Playback Target.
                  </p>

                  <Show when={quickConnectCode()}>
                    <div class="mt-6 inline-flex flex-col items-center justify-center px-6 py-3.5 rounded-2xl bg-surface-container-lowest/80 border border-outline-variant shadow-inner">
                      <span class="text-[10px] uppercase tracking-[0.2em] text-on-surface-variant/80 mb-1 font-bold">
                        Verification Code
                      </span>
                      <p class="font-mono text-display-small tracking-[0.25em] text-secondary drop-shadow-[0_0_10px_rgba(129,140,248,0.55)] pl-[0.25em]">
                        {quickConnectCode()}
                      </p>
                    </div>
                  </Show>

                  <Show when={isQuickConnectWaiting()}>
                    <div class="mt-5 flex items-center justify-center gap-2 text-label-medium text-secondary animate-pulse">
                      <span class="w-2 h-2 rounded-full bg-secondary shadow-[0_0_8px_#818cf8]" />
                      Awaiting Quick Connect Approval…
                    </div>
                  </Show>
                </div>
              </Tabs.Content>

              <Tabs.Content value="password">
                <div class="space-y-4">
                  <form.Field name="username">
                    {(field) => (
                      <ArkField.Root class="block">
                        <ArkField.Label class="mb-1.5 block text-label-medium">
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
                        <ArkField.Label class="mb-1.5 block text-label-medium">
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
                        class="ark-checkbox text-body-medium text-on-surface mt-2.5"
                      >
                        <Checkbox.Control class="ark-checkbox__control">
                          <Checkbox.Indicator class="ark-checkbox__indicator">
                            <Check class="h-3.5 w-3.5" stroke-width={4} />
                          </Checkbox.Indicator>
                        </Checkbox.Control>
                        <Checkbox.Label class="cursor-pointer font-medium hover:text-on-surface-variant transition-colors select-none">
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
                class="flex items-start gap-3 rounded-2xl bg-error-container/20 border border-error/30 p-4 text-on-error-container"
                role="alert"
              >
                <CircleAlert class="mt-0.5 h-5 w-5 text-error shrink-0" />
                <div>
                  <p class="text-title-small text-error font-bold">
                    Connection needs attention
                  </p>
                  <p class="text-body-medium mt-0.5">{error()}</p>
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
