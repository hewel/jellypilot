import { CircleAlert, Plus, Server, UserRound } from 'lucide-solid';
import { For, Show } from 'solid-js';

import type { SavedServiceProfiles } from '../../bindings';
import { Button, SectionCard } from '../ui';

import * as patterns from '../../styles/patterns.css';
import * as styles from './SavedServicesCard.css';
import * as shared from './shared.css';

interface SavedServicesCardProps {
  profiles: SavedServiceProfiles | null;
  activatingProfileKey: string | null;
  removingProfileKey: string | null;
  onAddService: () => void;
  onActivateProfile: (key: string) => void;
  onRemoveProfile: (key: string) => void;
}

export default function SavedServicesCard(props: SavedServicesCardProps) {
  const profiles = () => props.profiles?.profiles ?? [];

  return (
    <SectionCard icon={<Server class={shared.sectionIcon.secondary} />} title="Saved Services">
      <div class={styles.stack}>
        <Show
          when={profiles().length > 0}
          fallback={
            <div class={styles.profile}>
              <p class={styles.name}>No saved services yet</p>
              <p class={shared.bodyText}>
                Add a Jellyfin or Emby service to keep it available for switching.
              </p>
            </div>
          }
        >
          <For each={profiles()}>
            {(profile) => (
              <div
                class={styles.profile}
                classList={{
                  [styles.activeProfile]: profile.active,
                  [styles.warningProfile]: Boolean(profile.lastRestoreError),
                }}
              >
                <div class={styles.profileInner}>
                  <div class={styles.copy}>
                    <div class={styles.titleRow}>
                      <p class={styles.name}>{profile.serverName ?? profile.serverUrl}</p>
                      <span class={styles.pill}>{profile.provider}</span>
                      <Show when={profile.active}>
                        <span class={`${styles.pill} ${styles.activePill}`}>Active</span>
                      </Show>
                    </div>
                    <p class={styles.url}>{profile.serverUrl}</p>
                    <p class={styles.user}>
                      <UserRound class={patterns.icon3_5} />
                      {profile.userName}
                    </p>
                    <Show when={profile.lastRestoreError}>
                      {(message) => (
                        <p class={styles.warning}>
                          <CircleAlert class={styles.warningIcon} />
                          <span>{message()}</span>
                        </p>
                      )}
                    </Show>
                  </div>
                  <div class={styles.actions}>
                    <Show when={!profile.active}>
                      <Button
                        type="button"
                        variant="secondary"
                        disabled={props.activatingProfileKey === profile.key}
                        onClick={() => props.onActivateProfile(profile.key)}
                      >
                        {props.activatingProfileKey === profile.key ? 'Switching...' : 'Activate'}
                      </Button>
                    </Show>
                    <Button
                      type="button"
                      variant="outlined"
                      class={styles.dangerButton}
                      disabled={props.removingProfileKey === profile.key}
                      onClick={() => props.onRemoveProfile(profile.key)}
                    >
                      {props.removingProfileKey === profile.key ? 'Removing...' : 'Remove'}
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>

      <div class={styles.footer}>
        <Button
          type="button"
          variant="primary"
          onClick={props.onAddService}
          leadingIcon={<Plus class={patterns.icon4_5} />}
        >
          Add service
        </Button>
      </div>
    </SectionCard>
  );
}
