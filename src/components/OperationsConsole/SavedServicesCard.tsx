import { CircleAlert, Plus, Server, UserRound } from 'lucide-solid';
import { For, Show } from 'solid-js';

import type { SavedServiceProfiles } from '../../bindings';
import { Button, SectionCard } from '../ui';

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
    <SectionCard
      icon={<Server class="text-secondary h-5 w-5 drop-shadow-[0_0_8px_rgba(129,140,248,0.4)]" />}
      title="Saved Services"
    >
      <div class="space-y-3">
        <Show
          when={profiles().length > 0}
          fallback={
            <div class="border-outline-variant bg-surface-container-high/30 rounded-2xl border p-4">
              <p class="text-on-surface text-[14px] leading-[20px] font-semibold">
                No saved services yet
              </p>
              <p class="text-on-surface-variant mt-1 text-[12px] leading-[16px]">
                Add a Jellyfin or Emby service to keep it available for switching.
              </p>
            </div>
          }
        >
          <For each={profiles()}>
            {(profile) => (
              <div
                class="border-outline-variant bg-surface-container-high/30 rounded-2xl border p-4"
                classList={{
                  'border-secondary/70 shadow-[0_0_0_1px_rgba(129,140,248,0.25)]': profile.active,
                  'border-warning/60': Boolean(profile.lastRestoreError),
                }}
              >
                <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                      <p class="text-on-surface text-[15px] leading-[22px] font-bold">
                        {profile.serverName ?? profile.serverUrl}
                      </p>
                      <span class="border-outline-variant text-on-surface-variant rounded-full border px-2 py-0.5 text-[10px] leading-[14px] font-bold tracking-[0.08em] uppercase">
                        {profile.provider}
                      </span>
                      <Show when={profile.active}>
                        <span class="bg-secondary/15 text-secondary rounded-full px-2 py-0.5 text-[10px] leading-[14px] font-bold tracking-[0.08em] uppercase">
                          Active
                        </span>
                      </Show>
                    </div>
                    <p class="text-secondary mt-1 truncate font-mono text-[12px] leading-[16px]">
                      {profile.serverUrl}
                    </p>
                    <p class="text-on-surface-variant mt-1 flex items-center gap-1.5 text-[12px] leading-[16px]">
                      <UserRound class="h-3.5 w-3.5" />
                      {profile.userName}
                    </p>
                    <Show when={profile.lastRestoreError}>
                      {(message) => (
                        <p class="text-warning mt-2 flex items-start gap-1.5 text-[12px] leading-[16px] font-semibold">
                          <CircleAlert class="mt-0.5 h-3.5 w-3.5 shrink-0" />
                          <span>{message()}</span>
                        </p>
                      )}
                    </Show>
                  </div>
                  <div class="flex shrink-0 flex-wrap gap-2">
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
                      class="border-error/55 text-error hover:bg-error/10 hover:border-error"
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

      <div class="mt-5">
        <Button
          type="button"
          variant="primary"
          onClick={props.onAddService}
          leadingIcon={<Plus class="h-4.5 w-4.5" />}
        >
          Add service
        </Button>
      </div>
    </SectionCard>
  );
}
