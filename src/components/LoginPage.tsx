import { Checkbox } from '@ark-ui/solid/checkbox';
import { Tabs } from '@ark-ui/solid/tabs';
import { createForm } from '@tanstack/solid-form';
import { useQueryClient } from '@tanstack/solid-query';
import { Effect, Exit, Fiber } from 'effect';
import { Check, CircleAlert, LoaderCircle, RadioTower } from 'lucide-solid';
import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';

import type { Credentials, MediaServerProvider } from '../bindings';
import { commandFailureMessage } from '../effects/commands';
import { connectJellyfin } from '../effects/connection';
import { CommandError } from '../effects/errors';
import { queryKeys } from '../effects/query';
import { runQuickConnectWorkflow } from '../effects/quickConnect';
import { clearSavedCredentials, loadSavedCredentials, saveCredentials } from '../effects/session';
import { capabilitiesForProvider } from '../providerCapabilities';
import {
  buildServerUrlEffect,
  defaultSchemeForHost,
  explicitSchemeFromInput,
  parseServerUrl,
  stripServerScheme,
} from '../serverUrl';
import type { ServerScheme, ServerUrlResult } from '../serverUrl';
import { saveCurrentSession } from '../sessionAccess';
import { Button, Card, ConsoleShell, Field, FieldControl, PageFooter } from './ui';

import * as patterns from '../styles/patterns.css';
import * as styles from './LoginPage.css';

interface LoginPageProps {
  onConnected: () => void;
  embedded?: boolean;
}
type LoginMethod = 'quickConnect' | 'password';
type QuickConnectState = 'idle' | 'waiting' | 'failed';

interface LoginValues {
  provider: MediaServerProvider;
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
  const queryClient = useQueryClient();
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
      provider: 'jellyfin' as MediaServerProvider,
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
  const selectedCapabilities = () => capabilitiesForProvider(formValues().provider);
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
    queryClient.removeQueries({ queryKey: queryKeys.libraryRoot });
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
      queryClient.removeQueries({ queryKey: queryKeys.libraryRoot });
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
      provider: value.provider,
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
            if (value.rememberMe)
              Effect.runSync(saveCredentials(finalServerUrl, value.username, value.provider));
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

  createEffect(() => {
    if (!selectedCapabilities().quickConnect && loginMethod() === 'quickConnect') {
      resetQuickConnect();
      setLoginMethod('password');
    }
  });

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
      provider: saved.provider,
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

  const loginCard = () => (
    <Card variant="elevated" class={styles.card}>
      <div class={styles.accent} />
      <div class={styles.stack7}>
        <div>
          <h2 class={styles.sectionTitle}>
            <span class={styles.titleBar} />
            Server coordinates
          </h2>
          <p class={styles.description}>
            Choose the protocol and host. JellyPilot shows the final Server URL before any Login
            Method starts.
          </p>
        </div>

        <div class={styles.serverGrid}>
          <form.Field name="scheme">
            {(field) => (
              <fieldset
                class={`${styles.segmented} ${styles.segmented2}`}
                aria-label="Server protocol"
              >
                <button
                  type="button"
                  aria-pressed={field().state.value === 'https'}
                  class={styles.segment}
                  classList={{
                    [styles.segmentSelected]: field().state.value === 'https',
                    [styles.segmentIdle]: field().state.value !== 'https',
                  }}
                  disabled={isQuickConnectWaiting()}
                  onClick={() => field().handleChange('https')}
                >
                  HTTPS
                </button>
                <button
                  type="button"
                  aria-pressed={field().state.value === 'http'}
                  class={styles.segment}
                  classList={{
                    [styles.segmentSelected]: field().state.value === 'http',
                    [styles.segmentIdle]: field().state.value !== 'http',
                  }}
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
              <Field.Root class={styles.fieldBlock} disabled={isQuickConnectWaiting()}>
                <Field.Label class={styles.srOnly}>Jellyfin host</Field.Label>
                <Field.Input
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
                        form.setFieldValue('scheme', explicitScheme ?? defaultSchemeForHost(value));
                      }}
                      class={patterns.fullWidth}
                      placeholder="jellyfin.local or media.example.com/jellyfin"
                    />
                  )}
                />
              </Field.Root>
            )}
          </form.Field>
        </div>

        <div class={styles.preview}>
          <div class={styles.previewStripe} />
          <p class={styles.overline}>Server URL preview</p>
          <p
            class={styles.previewValue}
            classList={{
              [styles.previewReady]: Boolean(serverUrl()),
              [styles.previewEmpty]: !serverUrl(),
            }}
          >
            {serverUrl() || 'Enter a server host to preview the final URL'}
          </p>
        </div>

        <form.Field name="provider">
          {(field) => (
            <fieldset>
              <legend class={styles.label}>Media Server</legend>
              <div
                class={`${styles.segmented} ${styles.segmented2}`}
                aria-label="Media server provider"
              >
                <button
                  type="button"
                  class={styles.segment}
                  classList={{
                    [styles.segmentSelected]: field().state.value === 'jellyfin',
                    [styles.segmentIdle]: field().state.value !== 'jellyfin',
                  }}
                  disabled={isQuickConnectWaiting()}
                  onClick={() => field().handleChange('jellyfin')}
                >
                  Jellyfin
                </button>
                <button
                  type="button"
                  class={styles.segment}
                  classList={{
                    [styles.segmentSelected]: field().state.value === 'emby',
                    [styles.segmentIdle]: field().state.value !== 'emby',
                  }}
                  disabled={isQuickConnectWaiting()}
                  onClick={() => field().handleChange('emby')}
                >
                  Emby
                </button>
              </div>
            </fieldset>
          )}
        </form.Field>

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
            class={`${styles.segmented} ${styles.tabsList} ${selectedCapabilities().quickConnect ? styles.segmented2 : styles.segmented1}`}
            aria-label="Login Method"
          >
            <Show when={selectedCapabilities().quickConnect}>
              <Tabs.Trigger
                value="quickConnect"
                disabled={isQuickConnectWaiting()}
                class={styles.segment}
                classList={{
                  [styles.segmentSelected]: loginMethod() === 'quickConnect',
                  [styles.segmentIdle]: loginMethod() !== 'quickConnect',
                }}
              >
                Quick Connect
              </Tabs.Trigger>
            </Show>
            <Tabs.Trigger
              value="password"
              disabled={isQuickConnectWaiting()}
              class={styles.segment}
              classList={{
                [styles.segmentSelected]: loginMethod() === 'password',
                [styles.segmentIdle]: loginMethod() !== 'password',
              }}
            >
              Password
            </Tabs.Trigger>
          </Tabs.List>

          <Show when={selectedCapabilities().quickConnect}>
            <Tabs.Content value="quickConnect">
              <div class={styles.quickPanel}>
                <div class={styles.quickPanelGlow} />

                {/* Decorative radar background */}
                <div class={styles.radar}>
                  <Show when={isQuickConnectWaiting()}>
                    <div class={styles.radarRing} />
                    <div class={`${styles.radarRing} ${styles.radarRing2}`} />
                    <div class={`${styles.radarRing} ${styles.radarRing3}`} />
                  </Show>
                  <RadioTower
                    class={styles.towerIcon}
                    classList={{ [styles.pulse]: isQuickConnectWaiting() }}
                  />
                </div>

                <p class={styles.quickText}>
                  Approve this code from another signed-in Jellyfin client. JellyPilot will finish
                  login automatically after approval.
                </p>
                <p class={styles.quickHint}>You are authorizing this Playback Target.</p>

                <Show when={quickConnectCode()}>
                  <div class={styles.codeBox}>
                    <span class={styles.codeLabel}>Verification Code</span>
                    <p class={styles.code}>{quickConnectCode()}</p>
                  </div>
                </Show>

                <Show when={isQuickConnectWaiting()}>
                  <div class={styles.waiting}>
                    <span class={styles.waitingDot} />
                    Awaiting Quick Connect Approval…
                  </div>
                </Show>
              </div>
            </Tabs.Content>
          </Show>

          <Tabs.Content value="password">
            <div class={styles.stack4}>
              <form.Field name="username">
                {(field) => (
                  <Field.Root class={styles.fieldBlock}>
                    <Field.Label class={styles.label}>Username</Field.Label>
                    <Field.Input
                      asChild={(fieldProps) => (
                        <FieldControl
                          {...fieldProps()}
                          variant="filled"
                          value={field().state.value}
                          onInput={(event) => field().handleChange(event.currentTarget.value)}
                          class={patterns.fullWidth}
                          placeholder="Jellyfin username"
                        />
                      )}
                    />
                  </Field.Root>
                )}
              </form.Field>
              <form.Field name="password">
                {(field) => (
                  <Field.Root class={styles.fieldBlock}>
                    <Field.Label class={styles.label}>Password</Field.Label>
                    <Field.Input
                      asChild={(fieldProps) => (
                        <FieldControl
                          {...fieldProps()}
                          variant="filled"
                          type="password"
                          value={field().state.value}
                          onInput={(event) => field().handleChange(event.currentTarget.value)}
                          class={patterns.fullWidth}
                          placeholder="Jellyfin password"
                        />
                      )}
                    />
                  </Field.Root>
                )}
              </form.Field>
              <form.Field name="rememberMe">
                {(field) => (
                  <Checkbox.Root
                    checked={field().state.value}
                    onCheckedChange={(details) => field().handleChange(details.checked === true)}
                    class={styles.remember}
                  >
                    <Checkbox.Control class={styles.checkbox}>
                      <Checkbox.Indicator class={styles.checkboxIndicator}>
                        <Check class={patterns.icon3_5} stroke-width={4} />
                      </Checkbox.Indicator>
                    </Checkbox.Control>
                    <Checkbox.Label class={styles.checkboxLabel}>
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
          <div class={styles.alert} role="alert">
            <CircleAlert class={styles.alertIcon} />
            <div>
              <p class={styles.alertTitle}>Connection needs attention</p>
              <p class={styles.alertMessage}>{error()}</p>
            </div>
          </div>
        </Show>

        {isQuickConnectWaiting() ? (
          <Button
            type="button"
            variant="secondary"
            class={patterns.fullWidth}
            onClick={resetQuickConnect}
          >
            Cancel Request
          </Button>
        ) : (
          <Button
            type="button"
            disabled={submitting()}
            variant="primary"
            class={patterns.fullWidth}
            onClick={submit}
          >
            {submitting() ? (
              <>
                <LoaderCircle class={`${patterns.icon5} ${patterns.spinner}`} />
                {submittingButtonLabel()}
              </>
            ) : (
              submitButtonLabel()
            )}
          </Button>
        )}
      </div>
    </Card>
  );

  if (props.embedded) {
    return loginCard();
  }

  return (
    <ConsoleShell class={styles.shell}>
      <main class={styles.main}>
        <div class={styles.hero}>
          <div class={styles.heroIconWrap}>
            <div class={styles.heroRing} />
            <div class={styles.heroPulseRing} />
            <div class={styles.heroOrbit} />
            <RadioTower class={styles.heroIcon} />
          </div>

          <div class={styles.badge}>
            <span class={styles.badgeDot} />
            <p class={styles.badgeText}>Docking Sequence</p>
          </div>

          <h1 class={styles.appTitle}>JellyPilot</h1>
          <p class={styles.appDescription}>
            Connect this Playback Target to a known Jellyfin server.
          </p>
        </div>

        {loginCard()}

        <PageFooter class={styles.footer} />
      </main>
    </ConsoleShell>
  );
}
