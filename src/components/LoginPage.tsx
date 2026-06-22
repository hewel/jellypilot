import { Checkbox } from '@ark-ui/solid/checkbox';
import { Field as ArkField } from '@ark-ui/solid/field';
import { Tabs } from '@ark-ui/solid/tabs';
import { createForm } from '@tanstack/solid-form';
import { Effect, Exit, Fiber } from 'effect';
import { Check, CircleAlert, LoaderCircle, RadioTower } from 'lucide-solid';
import { Show, createSignal, onCleanup, onMount } from 'solid-js';

import type { Credentials } from '../bindings';
import { commandFailureMessage } from '../effects/commands';
import { connectJellyfin } from '../effects/connection';
import { CommandError } from '../effects/errors';
import { runQuickConnectWorkflow } from '../effects/quickConnect';
import { clearSavedCredentials, loadSavedCredentials, saveCredentials } from '../effects/session';
import {
  buildServerUrlEffect,
  defaultSchemeForHost,
  explicitSchemeFromInput,
  parseServerUrl,
  stripServerScheme,
} from '../serverUrl';
import type { ServerScheme, ServerUrlResult } from '../serverUrl';
import { saveCurrentSession } from '../sessionAccess';
import { Button, Card, ConsoleShell, FieldControl, PageFooter } from './ui';

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
  const [loginMethod, setLoginMethod] = createSignal<LoginMethod>('quickConnect');
  const [quickConnectState, setQuickConnectState] = createSignal<QuickConnectState>('idle');
  const [quickConnectCode, setQuickConnectCode] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  let quickConnectFiber: Fiber.Fiber<void, CommandError> | undefined;

  const form = createForm(() => ({
    defaultValues: {
      host: '',
      password: '',
      rememberMe: false,
      scheme: 'https' as ServerScheme,
      username: '',
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
  const submitButtonLabel = () => {
    if (loginMethod() !== 'quickConnect') {
      return 'Connect';
    }
    return quickConnectState() === 'failed' ? 'Request a new code' : 'Request Quick Connect code';
  };
  const submittingButtonLabel = () =>
    loginMethod() === 'quickConnect' ? 'Requesting...' : 'Connecting...';

  const validateServerUrl = (value: Pick<LoginValues, 'scheme' | 'host'>): ServerUrlValidation =>
    Effect.runSync(
      buildServerUrlEffect({
        host: value.host,
        scheme: value.scheme,
      }).pipe(
        Effect.match({
          onFailure: (err) => ({ message: err.message, status: 'error' }),
          onSuccess: (result) => ({ result, status: 'ok' }),
        }),
      ),
    );

  const serverUrlResult = () => {
    const validation = validateServerUrl({
      host: formValues().host,
      scheme: formValues().scheme,
    });
    return validation.status === 'ok' ? validation.result : null;
  };

  const serverUrl = () => serverUrlResult()?.url ?? '';

  const resetQuickConnect = () => {
    if (quickConnectFiber) {
      void Effect.runPromise(Fiber.interrupt(quickConnectFiber));
      quickConnectFiber = undefined;
    }
    setQuickConnectState('idle');
    setQuickConnectCode(null);
    setError(null);
    setSubmitting(false);
  };

  const finishConnected = async () => {
    await saveCurrentSession();
    props.onConnected();
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

    if (quickConnectFiber) {
      await Effect.runPromise(Fiber.interrupt(quickConnectFiber));
    }

    const fiber = Effect.runFork(
      runQuickConnectWorkflow(serverUrlValue, (code) => {
        setQuickConnectCode(code);
        setQuickConnectState('waiting');
        setSubmitting(false);
      }),
    );
    quickConnectFiber = fiber;

    const exit = await Effect.runPromiseExit(Fiber.join(fiber));

    if (quickConnectFiber !== fiber) {
      return;
    }
    quickConnectFiber = undefined;
    setSubmitting(false);

    if (Exit.isSuccess(exit)) {
      props.onConnected();
    } else {
      setQuickConnectState('failed');
      setError(commandFailureMessage(exit.cause, 'Quick Connect failed'));
    }
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
      password: value.password,
      serverUrl: finalServerUrl,
      username: value.username,
    };

    setSubmitting(true);
    const exit = await Effect.runPromiseExit(connectJellyfin(credentials));

    if (Exit.isSuccess(exit)) {
      const completion = await Effect.runPromiseExit(
        Effect.tryPromise({
          catch: (error) =>
            new CommandError({
              message: error instanceof Error ? error.message : 'Connection failed',
            }),
          try: async () => {
            if (value.rememberMe) Effect.runSync(saveCredentials(finalServerUrl, value.username));
            else Effect.runSync(clearSavedCredentials());
            await finishConnected();
          },
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
    if (!Exit.isSuccess(exit)) {
      return;
    }
    const saved = exit.value;
    const parsed = parseServerUrl(saved.serverUrl);
    form.reset({
      host: parsed.host,
      password: '',
      rememberMe: saved.rememberMe,
      scheme: parsed.scheme,
      username: saved.username,
    });
  });

  onCleanup(() => {
    if (quickConnectFiber) {
      void Effect.runPromise(Fiber.interrupt(quickConnectFiber));
      quickConnectFiber = undefined;
    }
  });

  return (
    <ConsoleShell class="relative flex items-center justify-center overflow-y-auto py-10">
      <main class="relative z-10 w-full max-w-3xl">
        <div class="relative mb-8 text-center">
          {/* Glowing HUD hologram decoration */}
          <div class="border-primary/20 bg-primary/5 relative mx-auto mb-6 flex h-24 w-24 items-center justify-center rounded-full border shadow-[0_0_30px_rgba(79,70,229,0.2)]">
            <div class="border-primary/30 absolute inset-0 animate-ping rounded-full border opacity-25" />
            <div class="border-secondary/25 absolute inset-2 animate-pulse rounded-full border" />
            <div class="border-primary/10 absolute inset-4 animate-[spin_60s_linear_infinite] rounded-full border border-dashed" />
            <RadioTower class="text-primary h-10 w-10 drop-shadow-[0_0_12px_rgba(79,70,229,0.55)]" />
          </div>

          <div class="border-secondary/20 bg-secondary/5 mb-3.5 inline-flex items-center gap-2.5 rounded-full border px-3.5 py-1">
            <span class="bg-secondary h-1.5 w-1.5 animate-pulse rounded-full shadow-[0_0_8px_#818cf8]" />
            <p class="text-secondary text-[10px] font-bold tracking-[0.18em] uppercase">
              Docking Sequence
            </p>
          </div>

          <h1 class="font-display text-on-surface text-[45px] leading-[52px] font-bold tracking-tight">
            JellyPilot
          </h1>
          <p class="text-on-surface-variant mx-auto mt-2 max-w-md text-[16px] leading-[24px]">
            Connect this Playback Target to a known Jellyfin server.
          </p>
        </div>

        <Card variant="elevated" class="relative mx-auto overflow-hidden shadow-2xl">
          <div class="via-primary/55 absolute top-0 left-0 h-[2px] w-full bg-gradient-to-r from-transparent to-transparent" />
          <div class="space-y-7">
            <div>
              <h2 class="font-display text-on-surface flex items-center gap-2.5 text-[24px] leading-[32px] font-bold tracking-tight">
                <span class="bg-primary h-5 w-1.5 rounded" />
                Server coordinates
              </h2>
              <p class="text-on-surface-variant mt-1.5 text-[14px] leading-[20px]">
                Choose the protocol and host. JellyPilot shows the final Server URL before any Login
                Method starts.
              </p>
            </div>

            <div class="grid grid-cols-1 gap-3 sm:grid-cols-[auto_minmax(0,1fr)]">
              <form.Field name="scheme">
                {(field) => (
                  <fieldset
                    class="border-outline-variant bg-surface-container-high/40 grid grid-cols-2 rounded-2xl border p-1"
                    aria-label="Server protocol"
                  >
                    <button
                      type="button"
                      class={`cursor-pointer rounded-xl px-4 py-3 text-[14px] leading-[20px] font-semibold tracking-wide uppercase transition-[background-color,color,box-shadow,transform] duration-300 active:scale-[0.96] ${field().state.value === 'https' ? 'bg-primary from-primary to-primary-gradient-end text-on-primary shadow-primary/20 bg-gradient-to-r font-bold shadow-md' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
                      disabled={isQuickConnectWaiting()}
                      onClick={() => field().handleChange('https')}
                    >
                      HTTPS
                    </button>
                    <button
                      type="button"
                      class={`cursor-pointer rounded-xl px-4 py-3 text-[14px] leading-[20px] font-semibold tracking-wide uppercase transition-[background-color,color,box-shadow,transform] duration-300 active:scale-[0.96] ${field().state.value === 'http' ? 'bg-primary from-primary to-primary-gradient-end text-on-primary shadow-primary/20 bg-gradient-to-r font-bold shadow-md' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
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
                  <ArkField.Root class="block" disabled={isQuickConnectWaiting()}>
                    <ArkField.Label class="sr-only">Jellyfin host</ArkField.Label>
                    <ArkField.Input
                      asChild={(fieldProps) => (
                        <FieldControl
                          {...fieldProps()}
                          variant="filled"
                          type="text"
                          value={field().state.value}
                          onInput={(event) => {
                            const { value } = event.currentTarget;
                            const explicitScheme = explicitSchemeFromInput(value);
                            const strippedHost = stripServerScheme(value);
                            field().handleChange(strippedHost);
                            form.setFieldValue(
                              'scheme',
                              explicitScheme ?? defaultSchemeForHost(value),
                            );
                          }}
                          class="w-full"
                          placeholder="jellyfin.local or media.example.com/jellyfin"
                        />
                      )}
                    />
                  </ArkField.Root>
                )}
              </form.Field>
            </div>

            <div class="border-outline-variant bg-surface-container-lowest/40 relative overflow-hidden rounded-2xl border p-4 backdrop-blur-sm">
              <div class="bg-secondary absolute inset-y-0 left-0 w-[3px]" />
              <p class="text-on-surface-variant/90 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                Server URL preview
              </p>
              <p
                class={`text-on-surface-variant mt-1 font-mono text-[14px] leading-[20px] break-all ${serverUrl() ? 'text-secondary font-semibold drop-shadow-[0_0_8px_rgba(129,140,248,0.15)]' : 'text-warning'}`}
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
                if (value !== 'quickConnect' && value !== 'password') {
                  return;
                }
                resetQuickConnect();
                setLoginMethod(value);
              }}
            >
              <Tabs.List
                class="border-outline-variant bg-surface-container-high/40 mb-6 grid grid-cols-2 rounded-2xl border p-1"
                aria-label="Login Method"
              >
                <Tabs.Trigger
                  value="quickConnect"
                  disabled={isQuickConnectWaiting()}
                  class={`cursor-pointer rounded-xl px-4 py-3 text-[14px] leading-[20px] font-semibold tracking-wide uppercase transition-[background-color,color,box-shadow,transform] duration-300 active:scale-[0.96] ${loginMethod() === 'quickConnect' ? 'from-primary to-primary-gradient-end text-on-primary shadow-primary/25 bg-gradient-to-r font-bold shadow-lg' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
                >
                  Quick Connect
                </Tabs.Trigger>
                <Tabs.Trigger
                  value="password"
                  disabled={isQuickConnectWaiting()}
                  class={`cursor-pointer rounded-xl px-4 py-3 text-[14px] leading-[20px] font-semibold tracking-wide uppercase transition-[background-color,color,box-shadow,transform] duration-300 active:scale-[0.96] ${loginMethod() === 'password' ? 'from-primary to-primary-gradient-end text-on-primary shadow-primary/25 bg-gradient-to-r font-bold shadow-lg' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest/40'}`}
                >
                  Password
                </Tabs.Trigger>
              </Tabs.List>

              <Tabs.Content value="quickConnect">
                <div class="border-secondary/25 bg-secondary-container/20 relative overflow-hidden rounded-3xl border p-6 text-center backdrop-blur-sm transition-colors">
                  <div class="from-secondary/5 pointer-events-none absolute inset-0 bg-gradient-to-b to-transparent" />

                  {/* Decorative radar background */}
                  <div class="border-secondary/20 bg-secondary/5 relative mx-auto mb-4 flex h-20 w-20 items-center justify-center rounded-full border">
                    <Show when={isQuickConnectWaiting()}>
                      <div class="border-secondary/40 absolute inset-0 animate-[radar-pulse_2.2s_cubic-bezier(0.2,0.8,0.2,1)_infinite] rounded-full border" />
                      <div
                        class="border-secondary/30 absolute inset-0 animate-[radar-pulse_2.2s_cubic-bezier(0.2,0.8,0.2,1)_infinite] rounded-full border"
                        style="animation-delay: 0.7s"
                      />
                      <div
                        class="border-secondary/20 absolute inset-0 animate-[radar-pulse_2.2s_cubic-bezier(0.2,0.8,0.2,1)_infinite] rounded-full border"
                        style="animation-delay: 1.4s"
                      />
                    </Show>
                    <RadioTower
                      class={`text-secondary h-9 w-9 ${isQuickConnectWaiting() ? 'animate-pulse' : ''} drop-shadow-[0_0_8px_rgba(129,140,248,0.4)]`}
                    />
                  </div>

                  <p class="text-on-secondary-container text-[14px] leading-[20px] font-medium">
                    Approve this code from another signed-in Jellyfin client. JellyPilot will finish
                    login automatically after approval.
                  </p>
                  <p class="text-on-surface-variant/80 mt-2 text-[12px] leading-[16px]">
                    You are authorizing this Playback Target.
                  </p>

                  <Show when={quickConnectCode()}>
                    <div class="bg-surface-container-lowest/80 border-outline-variant mt-6 inline-flex flex-col items-center justify-center rounded-2xl border px-6 py-3.5 shadow-inner">
                      <span class="text-on-surface-variant/80 mb-1 text-[10px] font-bold tracking-[0.2em] uppercase">
                        Verification Code
                      </span>
                      <p class="font-display text-secondary pl-[0.25em] font-mono text-[36px] leading-[44px] font-bold tracking-[0.25em] tracking-tight tabular-nums drop-shadow-[0_0_10px_rgba(129,140,248,0.55)]">
                        {quickConnectCode()}
                      </p>
                    </div>
                  </Show>

                  <Show when={isQuickConnectWaiting()}>
                    <div class="text-secondary mt-5 flex animate-pulse items-center justify-center gap-2 text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                      <span class="bg-secondary h-2 w-2 rounded-full shadow-[0_0_8px_#818cf8]" />
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
                        <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                          Username
                        </ArkField.Label>
                        <ArkField.Input
                          asChild={(fieldProps) => (
                            <FieldControl
                              {...fieldProps()}
                              variant="filled"
                              value={field().state.value}
                              onInput={(event) => field().handleChange(event.currentTarget.value)}
                              class="w-full"
                              placeholder="Jellyfin username"
                            />
                          )}
                        />
                      </ArkField.Root>
                    )}
                  </form.Field>
                  <form.Field name="password">
                    {(field) => (
                      <ArkField.Root class="block">
                        <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                          Password
                        </ArkField.Label>
                        <ArkField.Input
                          asChild={(fieldProps) => (
                            <FieldControl
                              {...fieldProps()}
                              variant="filled"
                              type="password"
                              value={field().state.value}
                              onInput={(event) => field().handleChange(event.currentTarget.value)}
                              class="w-full"
                              placeholder="Jellyfin password"
                            />
                          )}
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
                        class="text-on-surface mt-2.5 inline-flex cursor-pointer items-center gap-2.5 align-top text-[14px] leading-[20px] transition-opacity select-none disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        <Checkbox.Control class="border-outline bg-surface-container-high text-on-primary hover:border-primary/60 data-[state=checked]:border-primary data-[state=checked]:from-primary data-[state=checked]:to-primary-gradient-end data-[state=indeterminate]:border-primary data-[state=indeterminate]:from-primary data-[state=indeterminate]:to-primary-gradient-end data-[focus-visible]:ring-primary/50 data-[focus-visible]:ring-offset-background inline-flex h-5.5 w-5.5 shrink-0 items-center justify-center rounded-lg border text-[11px] leading-none transition-[background-color,border-color,box-shadow] duration-200 data-[focus-visible]:ring-2 data-[focus-visible]:ring-offset-2 data-[focus-visible]:outline-none data-[state=checked]:bg-gradient-to-br data-[state=indeterminate]:bg-gradient-to-br">
                          <Checkbox.Indicator class="flex items-center justify-center font-black">
                            <Check class="h-3.5 w-3.5" stroke-width={4} />
                          </Checkbox.Indicator>
                        </Checkbox.Control>
                        <Checkbox.Label class="hover:text-on-surface-variant cursor-pointer font-medium transition-colors select-none">
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
                class="bg-error-container/20 border-error/30 text-on-error-container flex items-start gap-3 rounded-2xl border p-4"
                role="alert"
              >
                <CircleAlert class="text-error mt-0.5 h-5 w-5 shrink-0" />
                <div>
                  <p class="text-error text-[14px] leading-[20px] font-bold font-semibold">
                    Connection needs attention
                  </p>
                  <p class="text-on-surface-variant mt-0.5 text-[14px] leading-[20px]">{error()}</p>
                </div>
              </div>
            </Show>

            {isQuickConnectWaiting() ? (
              <Button type="button" variant="secondary" class="w-full" onClick={resetQuickConnect}>
                Cancel Request
              </Button>
            ) : (
              <Button
                type="button"
                disabled={submitting()}
                variant="primary"
                class="w-full"
                onClick={submit}
              >
                {submitting() ? (
                  <>
                    <LoaderCircle class="h-5 w-5 animate-spin" />
                    {submittingButtonLabel()}
                  </>
                ) : (
                  submitButtonLabel()
                )}
              </Button>
            )}
          </div>
        </Card>

        <PageFooter class="mt-8" />
      </main>
    </ConsoleShell>
  );
}
