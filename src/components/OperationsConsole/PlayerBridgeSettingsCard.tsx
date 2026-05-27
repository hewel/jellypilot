import { Collapsible } from '@ark-ui/solid/collapsible';
import type { ListCollection } from '@ark-ui/solid/collection';
import { Field as ArkField } from '@ark-ui/solid/field';
import { Select } from '@ark-ui/solid/select';
import { TagsInput } from '@ark-ui/solid/tags-input';
import { ChevronDown, Settings } from 'lucide-solid';
import { For, Show } from 'solid-js';
import { SectionCard } from '../ui';
import { useOperationsConsoleStore } from './store';
import {
  getSubtitleLanguageLabel,
  parseSubtitleLanguageInput,
} from './subtitleLanguages';
import type { OperationsConsoleForm } from './types';

interface PlayerBridgeSettingsCardProps {
  form: OperationsConsoleForm;
  subtitleLanguageSelectCollection: ListCollection<{
    value: string;
    label: string;
  }>;
  onSaveTextSetting: (
    field: 'deviceName' | 'mpvPath' | 'mpvArgs',
    value: string,
  ) => void;
  onDetectMpv: () => void;
  onAddSubtitleLanguageCodes: (codes: string[]) => void;
  onAddSubtitleLanguages: () => void;
  onRemoveSubtitleLanguage: (language: string) => void;
  onClearSubtitleLanguages: () => void;
  onMoveSubtitleLanguage: (index: number, direction: -1 | 1) => void;
}

export default function PlayerBridgeSettingsCard(
  props: PlayerBridgeSettingsCardProps,
) {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <SectionCard
      icon={<Settings class="h-6 w-6" />}
      title="Player Bridge settings"
      trailing={
        <Show when={ui.playerBridgeSaveStatus}>
          {(status) => (
            <span
              class={`text-label-small font-semibold ${status().type === 'error' ? 'text-error' : 'text-secondary'}`}
            >
              {status().text}
            </span>
          )}
        </Show>
      }
    >
      <div class="space-y-5">
        <props.form.Field
          name="deviceName"
          validators={{
            onBlur: ({ value }) =>
              !value.trim() ? 'Device name is required' : undefined,
          }}
        >
          {(field) => (
            <ArkField.Root
              class="block"
              invalid={field().state.meta.errors.length > 0}
            >
              <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                Playback Target name
              </ArkField.Label>
              <ArkField.Input
                id={field().name}
                name={field().name}
                type="text"
                value={field().state.value}
                onInput={(event) =>
                  field().handleChange(event.currentTarget.value)
                }
                onBlur={(event) => {
                  field().handleBlur();
                  props.onSaveTextSetting(
                    'deviceName',
                    event.currentTarget.value,
                  );
                }}
                class="input-filled w-full"
                placeholder="JMSR"
              />
              <Show when={field().state.meta.errors.length > 0}>
                <ArkField.ErrorText class="mt-1 text-body-small text-error">
                  {field().state.meta.errors[0]}
                </ArkField.ErrorText>
              </Show>
              <ArkField.HelperText class="mt-1 text-body-small text-on-surface-variant">
                Name displayed in Jellyfin cast menu.
              </ArkField.HelperText>
            </ArkField.Root>
          )}
        </props.form.Field>

        <props.form.Field name="mpvPath">
          {(field) => (
            <ArkField.Root class="block">
              <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                MPV executable path
              </ArkField.Label>
              <div class="flex flex-col gap-2 sm:flex-row">
                <ArkField.Input
                  id={field().name}
                  name={field().name}
                  type="text"
                  value={field().state.value}
                  onInput={(event) =>
                    field().handleChange(event.currentTarget.value)
                  }
                  onBlur={(event) => {
                    field().handleBlur();
                    props.onSaveTextSetting(
                      'mpvPath',
                      event.currentTarget.value,
                    );
                  }}
                  placeholder="Path to mpv executable"
                  class="input-filled min-w-0 flex-1"
                />
                <button
                  type="button"
                  onClick={props.onDetectMpv}
                  disabled={ui.detectingMpv}
                  class="btn-secondary"
                >
                  {ui.detectingMpv ? 'Detecting...' : 'Detect MPV'}
                </button>
              </div>
            </ArkField.Root>
          )}
        </props.form.Field>

        <Collapsible.Root
          open={ui.advancedOpen}
          onOpenChange={(details) => actions.setAdvancedOpen(details.open)}
          lazyMount
          unmountOnExit
        >
          <Collapsible.Trigger class="btn-text px-0">
            <Collapsible.Indicator>
              <ChevronDown
                class={`h-5 w-5 transition-transform ${ui.advancedOpen ? 'rotate-180' : ''}`}
              />
            </Collapsible.Indicator>
            Advanced MPV options
          </Collapsible.Trigger>

          <Collapsible.Content class="rounded-3xl border border-outline-variant bg-surface-container-lowest p-4">
            <section class="space-y-3">
              <div>
                <h3 class="text-title-small text-on-surface">MPV arguments</h3>
                <p class="mt-1 text-body-small text-on-surface-variant">
                  Extra command-line flags passed to the external MPV process.
                </p>
              </div>

              <props.form.Field name="mpvArgs">
                {(field) => (
                  <ArkField.Root class="block">
                    <ArkField.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                      Extra arguments
                    </ArkField.Label>
                    <ArkField.Textarea
                      value={field().state.value}
                      onInput={(event) =>
                        field().handleChange(event.currentTarget.value)
                      }
                      onBlur={(event) => {
                        field().handleBlur();
                        props.onSaveTextSetting(
                          'mpvArgs',
                          event.currentTarget.value,
                        );
                      }}
                      rows={4}
                      placeholder="--fullscreen&#10;--force-window"
                      class="input-filled h-auto w-full py-3 font-mono text-body-small"
                    />
                  </ArkField.Root>
                )}
              </props.form.Field>
            </section>
          </Collapsible.Content>
        </Collapsible.Root>
        <TagsInput.Root
          value={ui.selectedSubtitleLanguages}
          inputValue=""
          editable={false}
          class="rounded-2xl bg-surface-container-high p-4"
        >
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 class="text-title-medium text-on-surface">
                Preferred subtitle languages
              </h3>
              <p class="mt-1 text-body-small text-on-surface-variant">
                Add Jellyfin language codes in fallback priority order.
              </p>
            </div>
            <Show when={ui.selectedSubtitleLanguages.length > 0}>
              <button
                type="button"
                class="btn-text min-w-0 px-3"
                onClick={props.onClearSubtitleLanguages}
              >
                Clear all
              </button>
              <TagsInput.ClearTrigger class="hidden" />
            </Show>
          </div>

          <div class="mt-4 flex flex-col gap-3 sm:flex-row">
            <Select.Root
              collection={props.subtitleLanguageSelectCollection}
              closeOnSelect
              onValueChange={(details) => {
                if (details.value.length > 0) {
                  props.onAddSubtitleLanguageCodes(details.value);
                }
              }}
              value={[]}
            >
              <Select.Label class="mb-1 block text-label-medium uppercase text-on-surface-variant">
                Predefined languages
              </Select.Label>
              <Select.Control class="select-filled flex w-full items-center">
                <Select.Trigger class="flex h-14 w-full items-center justify-between gap-2 rounded-2xl border border-outline/80 bg-surface-container-highest/70 px-4 text-on-surface outline-none transition-colors duration-200 hover:border-secondary/70 focus:border-secondary focus:ring-2 focus:ring-secondary/30">
                  <Select.ValueText
                    placeholder="Select a language…"
                    class="text-on-surface-variant/70"
                  />
                  <Select.Indicator>
                    <ChevronDown class="h-4 w-4 text-on-surface-variant" />
                  </Select.Indicator>
                </Select.Trigger>
              </Select.Control>
              <Select.Positioner>
                <Select.Content class="mt-2 rounded-2xl border border-outline-variant bg-surface-container-lowest p-2 shadow-lg shadow-black/30">
                  <For each={props.subtitleLanguageSelectCollection.items}>
                    {(item) => (
                      <Select.Item
                        item={item}
                        class="flex cursor-pointer items-center justify-between rounded-xl px-3 py-2 text-body-small text-on-surface-variant hover:bg-surface-container-high"
                      >
                        <Select.ItemText>{item.label}</Select.ItemText>
                      </Select.Item>
                    )}
                  </For>
                </Select.Content>
              </Select.Positioner>
              <Select.HiddenSelect />
            </Select.Root>

            <div class="flex min-w-0 flex-1 flex-col">
              <label
                for="custom-subtitle-lang-input"
                class="mb-1 block text-label-medium uppercase text-on-surface-variant"
              >
                Custom code
              </label>
              <div class="flex gap-2">
                <input
                  id="custom-subtitle-lang-input"
                  type="text"
                  value={ui.subtitleLanguageInput}
                  onInput={(event) =>
                    actions.setSubtitleLanguageInput(event.currentTarget.value)
                  }
                  onKeyDown={(event) => {
                    if (event.key !== 'Enter') return;
                    event.preventDefault();
                    props.onAddSubtitleLanguages();
                  }}
                  class="input-filled min-w-0 flex-1 font-mono"
                  placeholder="e.g. pol, tha"
                  aria-label="Custom subtitle language code"
                />
                <button
                  type="button"
                  class="inline-flex h-14 min-w-[5.5rem] items-center justify-center rounded-2xl bg-secondary-container px-4 text-[14px] leading-[20px] font-semibold text-on-secondary-container transition duration-200 hover:bg-secondary-container/80 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50"
                  disabled={
                    parseSubtitleLanguageInput(ui.subtitleLanguageInput)
                      .length === 0
                  }
                  onClick={props.onAddSubtitleLanguages}
                >
                  Add
                </button>
              </div>
            </div>
          </div>

          <Show
            when={ui.selectedSubtitleLanguages.length > 0}
            fallback={
              <p class="mt-4 rounded-2xl border border-dashed border-outline-variant px-4 py-3 text-body-small text-on-surface-variant">
                No preferred subtitle languages selected. JMSR will use Jellyfin
                and media defaults.
              </p>
            }
          >
            <ol
              class="mt-4 flex flex-wrap gap-2"
              aria-label="Selected preferred subtitle languages"
            >
              <For each={ui.selectedSubtitleLanguages}>
                {(language, index) => (
                  <TagsInput.Item
                    index={index()}
                    value={language}
                    class="inline-flex max-w-full items-center gap-2 rounded-full border border-outline-variant bg-surface-container-lowest px-3 py-2"
                  >
                    <TagsInput.ItemPreview class="contents">
                      <span class="text-label-small text-on-surface-variant">
                        {index() + 1}
                      </span>
                      <TagsInput.ItemText class="font-mono text-label-large text-on-surface">
                        {language}
                      </TagsInput.ItemText>
                      <span class="text-body-small text-on-surface-variant">
                        {getSubtitleLanguageLabel(language)}
                      </span>
                    </TagsInput.ItemPreview>
                    <button
                      type="button"
                      class="btn-text min-w-0 px-1"
                      disabled={index() === 0}
                      aria-label={`Move ${language} up`}
                      onClick={() => props.onMoveSubtitleLanguage(index(), -1)}
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      class="btn-text min-w-0 px-1"
                      disabled={
                        index() === ui.selectedSubtitleLanguages.length - 1
                      }
                      aria-label={`Move ${language} down`}
                      onClick={() => props.onMoveSubtitleLanguage(index(), 1)}
                    >
                      ↓
                    </button>
                    <TagsInput.ItemDeleteTrigger
                      class="btn-text min-w-0 px-1"
                      aria-label={`Remove ${language}`}
                      onClick={() => props.onRemoveSubtitleLanguage(language)}
                    >
                      Remove
                    </TagsInput.ItemDeleteTrigger>
                  </TagsInput.Item>
                )}
              </For>
            </ol>
          </Show>
          <TagsInput.HiddenInput />
        </TagsInput.Root>
      </div>
    </SectionCard>
  );
}
