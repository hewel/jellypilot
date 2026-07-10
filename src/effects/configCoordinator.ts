import type { AppConfig, ThemePreference } from '@bindings';
import type { Cause } from 'effect';
import { Option } from 'effect';

import { commandFailure } from './commands';
import { fetchConfig, saveConfig } from './config';
import { CommandError } from './errors';
import { runExit } from './query';

export type ConfigStatus = 'loading' | 'ready' | 'saving' | 'error';

export interface ConfigCoordinatorState {
  status: ConfigStatus;
  confirmed: AppConfig | null;
  desired: AppConfig | null;
  error: CommandError | null;
}

type Listener = (state: ConfigCoordinatorState) => void;

/**
 * One in-flight save; latest-intent-wins for pending patches.
 * Already-dispatched invokes are never interrupted.
 */
export class ConfigCoordinator {
  private state: ConfigCoordinatorState = {
    status: 'loading',
    confirmed: null,
    desired: null,
    error: null,
  };
  private listeners = new Set<Listener>();
  private saveInFlight = false;
  private pendingDesired: AppConfig | null = null;

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.state);
    return () => this.listeners.delete(listener);
  }

  getState(): ConfigCoordinatorState {
    return this.state;
  }

  async bootstrap(): Promise<void> {
    this.setState({ status: 'loading', error: null });
    const result = await runExit(fetchConfig());
    if (result._tag === 'Failure') {
      this.setState({
        status: 'error',
        confirmed: null,
        desired: null,
        error: toCommandError(result.cause),
      });
      return;
    }
    const config = withThemeDefault(result.value);
    this.setState({
      status: 'ready',
      confirmed: config,
      desired: config,
      error: null,
    });
  }

  async retry(): Promise<void> {
    await this.bootstrap();
  }

  setThemePreference(preference: ThemePreference): void {
    const base = this.state.desired ?? this.state.confirmed;
    if (!base) return;
    this.patch({ ...base, themePreference: preference });
  }

  patch(next: AppConfig): void {
    this.pendingDesired = withThemeDefault(next);
    this.setState({
      desired: this.pendingDesired,
      status: 'saving',
      error: null,
    });
    void this.drainSaves();
  }

  private async drainSaves(): Promise<void> {
    if (this.saveInFlight) return;
    while (this.pendingDesired) {
      const snapshot = this.pendingDesired;
      this.pendingDesired = null;
      this.saveInFlight = true;
      this.setState({ status: 'saving', desired: snapshot, error: null });
      const result = await runExit(saveConfig(snapshot));
      this.saveInFlight = false;
      if (result._tag === 'Failure') {
        if (this.pendingDesired) continue;
        this.setState({
          status: 'error',
          desired: this.state.confirmed,
          error: toCommandError(result.cause),
        });
        return;
      }
      if (this.pendingDesired) continue;
      this.setState({
        status: 'ready',
        confirmed: snapshot,
        desired: snapshot,
        error: null,
      });
    }
  }

  private setState(partial: Partial<ConfigCoordinatorState>): void {
    this.state = { ...this.state, ...partial };
    for (const listener of this.listeners) listener(this.state);
  }
}

function withThemeDefault(config: AppConfig | null | undefined): AppConfig {
  if (!config) {
    return { themePreference: 'system' };
  }
  return {
    ...config,
    themePreference: config.themePreference ?? 'system',
  };
}

function toCommandError(cause: Cause.Cause<CommandError>): CommandError {
  return Option.match(commandFailure(cause), {
    onNone: () =>
      new CommandError({
        code: 'internal',
        message: 'Configuration command failed',
      }),
    onSome: (error) => error,
  });
}

export function createConfigCoordinator(): ConfigCoordinator {
  return new ConfigCoordinator();
}
