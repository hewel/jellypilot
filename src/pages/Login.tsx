import { createForm } from '@tanstack/solid-form';
import { Check, CircleAlert, LoaderCircle } from 'lucide-solid';
import { createSignal, onMount, Show } from 'solid-js';
import { css, cx } from '../../styled-system/css';
import { button, input } from '../../styled-system/recipes';
import { type Credentials, commands } from '../bindings';
import { Card, PageFooter } from '../components/ui';
import { saveSession } from '../lib/auth';

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

export default function LoginPage(props: LoginPageProps) {
  const [error, setError] = createSignal<string | null>(null);

  const form = createForm(() => ({
    defaultValues: {
      serverUrl: '',
      username: '',
      password: '',
      rememberMe: false,
    },
    onSubmit: async ({ value }) => {
      setError(null);

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

          // Save the full session (with auth token) for auto-reconnect
          const session = await commands.jellyfinGetSession();
          if (session) {
            saveSession(session);
          }

          props.onConnected();
        } else {
          setError(result.error.message);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Connection failed');
      }
    },
  }));

  const isSubmitting = form.useStore((state) => state.isSubmitting);

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
    <div
      class={css({
        minHeight: '100vh',
        backgroundColor: 'background',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: '24px',
      })}
    >
      <div class={css({ width: '100%', maxWidth: '448px' })}>
        {/* Logo */}
        <div class={css({ textAlign: 'center', marginBottom: '40px' })}>
          <h1
            class={css({
              textStyle: 'displayMedium',
              fontWeight: 'normal',
              color: 'primary',
              letterSpacing: 'tight',
              textShadow: '0 1px 2px rgba(0,0,0,0.1)',
            })}
          >
            JMSR
          </h1>
          <p
            class={css({
              textStyle: 'bodyLarge',
              color: 'onSurfaceVariant',
              marginTop: '8px',
              letterSpacing: 'wide',
              fontWeight: 'normal',
            })}
          >
            Jellyfin MPV Shim
          </p>
        </div>

        {/* Login Card */}
        <Card
          variant="elevated"
          class={css({ position: 'relative', overflow: 'hidden' })}
        >
          {/* Surface Tint Overlay for Elevation 1 */}
          <div
            class={css({
              position: 'absolute',
              inset: 0,
              backgroundColor: 'primary/5',
              pointerEvents: 'none',
            })}
          />

          <div class={css({ position: 'relative', zIndex: 10 })}>
            <h2
              class={css({
                textStyle: 'headlineSmall',
                color: 'onSurface',
                marginBottom: '32px',
                letterSpacing: 'tight',
              })}
            >
              Connect to Server
            </h2>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                e.stopPropagation();
                form.handleSubmit();
              }}
              class={css({
                display: 'flex',
                flexDirection: 'column',
                gap: '24px',
              })}
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
                    <div
                      class={css({
                        _focusWithin: { '& label': { color: 'primary' } },
                      })}
                    >
                      <label
                        for={field().name}
                        class={css({
                          textStyle: 'labelMedium',
                          display: 'block',
                          color: 'onSurfaceVariant',
                          marginBottom: '4px',
                          marginLeft: '4px',
                          textTransform: 'uppercase',
                          letterSpacing: 'wider',
                          transition: 'colors',
                        })}
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
                        disabled={isSubmitting()}
                        class={cx(
                          input({ variant: 'filled' }),
                          css({ width: '100%' }),
                        )}
                      />
                      <Show when={touched() && errors().length > 0}>
                        <p
                          class={css({
                            color: 'error',
                            textStyle: 'bodySmall',
                            marginTop: '6px',
                            marginLeft: '4px',
                            animation:
                              'slideInFromTop 0.2s ease-out, fadeIn 0.2s ease-out',
                          })}
                        >
                          {errors()[0]}
                        </p>
                      </Show>
                    </div>
                  );
                }}
              />

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
                    <div
                      class={css({
                        _focusWithin: { '& label': { color: 'primary' } },
                      })}
                    >
                      <label
                        for={field().name}
                        class={css({
                          textStyle: 'labelMedium',
                          display: 'block',
                          color: 'onSurfaceVariant',
                          marginBottom: '4px',
                          marginLeft: '4px',
                          textTransform: 'uppercase',
                          letterSpacing: 'wider',
                          transition: 'colors',
                        })}
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
                        class={cx(
                          input({ variant: 'filled' }),
                          css({ width: '100%' }),
                        )}
                      />
                      <Show when={touched() && errors().length > 0}>
                        <p
                          class={css({
                            color: 'error',
                            textStyle: 'bodySmall',
                            marginTop: '6px',
                            marginLeft: '4px',
                            animation:
                              'slideInFromTop 0.2s ease-out, fadeIn 0.2s ease-out',
                          })}
                        >
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
                  <div
                    class={css({
                      _focusWithin: { '& label': { color: 'primary' } },
                    })}
                  >
                    <label
                      for={field().name}
                      class={css({
                        textStyle: 'labelMedium',
                        display: 'block',
                        color: 'onSurfaceVariant',
                        marginBottom: '4px',
                        marginLeft: '4px',
                        textTransform: 'uppercase',
                        letterSpacing: 'wider',
                        transition: 'colors',
                      })}
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
                      class={cx(
                        input({ variant: 'filled' }),
                        css({ width: '100%' }),
                      )}
                    />
                  </div>
                )}
              />

              {/* Remember Me */}
              <form.Field
                name="rememberMe"
                children={(field) => (
                  <div
                    class={css({
                      display: 'flex',
                      alignItems: 'center',
                      gap: '12px',
                      paddingY: '4px',
                    })}
                  >
                    <div
                      class={css({
                        position: 'relative',
                        display: 'flex',
                        alignItems: 'center',
                      })}
                    >
                      <input
                        id={field().name}
                        name={field().name}
                        type="checkbox"
                        checked={field().state.value}
                        onChange={(e) =>
                          field().handleChange(e.currentTarget.checked)
                        }
                        disabled={isSubmitting()}
                        class={css({
                          width: '20px',
                          height: '20px',
                          borderRadius: '4px',
                          borderWidth: '2px',
                          borderStyle: 'solid',
                          borderColor: 'onSurfaceVariant',
                          backgroundColor: 'transparent',
                          color: 'primary',
                          cursor: 'pointer',
                          appearance: 'none',
                          transition: 'all 0.2s',
                          _checked: {
                            borderColor: 'primary',
                            backgroundColor: 'primary',
                          },
                          _focus: {
                            ringWidth: '2px',
                            ringColor: 'primary',
                            ringOffset: '0px',
                          },
                        })}
                      />
                      <Check
                        class={css({
                          position: 'absolute',
                          pointerEvents: 'none',
                          opacity: field().state.value ? 1 : 0,
                          color: 'onPrimary',
                          width: '14px',
                          height: '14px',
                          left: '3px',
                          top: '3px',
                          transition: 'opacity 0.2s',
                        })}
                        stroke-width={4}
                      />
                    </div>
                    <label
                      for={field().name}
                      class={css({
                        textStyle: 'bodyMedium',
                        color: 'onSurface',
                        userSelect: 'none',
                        cursor: 'pointer',
                      })}
                    >
                      Remember server and username
                    </label>
                  </div>
                )}
              />

              {/* Error Message */}
              <Show when={error()}>
                <div
                  class={css({
                    padding: '16px',
                    backgroundColor: 'errorContainer',
                    color: 'onErrorContainer',
                    borderRadius: '12px',
                    animation:
                      'slideInFromTop 0.3s ease-out, fadeIn 0.3s ease-out',
                    display: 'flex',
                    alignItems: 'flex-start',
                    gap: '12px',
                  })}
                >
                  <div class={css({ marginTop: '2px' })}>
                    <CircleAlert
                      class={css({ width: '20px', height: '20px' })}
                    />
                  </div>
                  <p
                    class={css({
                      textStyle: 'bodyMedium',
                      fontWeight: 'medium',
                    })}
                  >
                    {error()}
                  </p>
                </div>
              </Show>

              {/* Submit Button */}
              <button
                type="submit"
                disabled={isSubmitting()}
                class={cx(
                  button({ variant: 'primary' }),
                  css({ width: '100%' }),
                )}
              >
                <Show when={isSubmitting()} fallback="Connect">
                  <LoaderCircle
                    class={css({
                      animation: 'spin 1s linear infinite',
                      height: '20px',
                      width: '20px',
                    })}
                  />
                  <span>Connecting...</span>
                </Show>
              </button>
            </form>
          </div>
        </Card>

        {/* Footer */}
        <PageFooter class={css({ marginTop: '32px' })} />
      </div>
    </div>
  );
}
