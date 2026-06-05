import { Collapsible } from '@ark-ui/solid/collapsible';
import type { ListCollection } from '@ark-ui/solid/collection';
import { Field as ArkField } from '@ark-ui/solid/field';
import { Select } from '@ark-ui/solid/select';
import { TagsInput } from '@ark-ui/solid/tags-input';
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  Globe,
  Plus,
  Settings,
  Trash2,
} from 'lucide-solid';
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
      icon={
        <Settings class="h-5 w-5 text-primary drop-shadow-[0_0_8px_rgba(165,1,219,0.4)]" />
      }
      title="Player Bridge settings"
      trailing={
        <Show when={ui.playerBridgeSaveStatus}>
          {(status) => (
            <span
              class={`text-[11px] font-bold uppercase tracking-wider px-2.5 py-0.5 rounded border ${
                status().type === 'error'
                  ? 'border-error/20 bg-error-container/20 text-error animate-pulse'
                  : 'border-secondary/20 bg-secondary-container/20 text-secondary'
              }`}
            >
              {status().text}
            </span>
          )}
        </Show>
      }
    >
      <div class="space-y-6">
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
              <ArkField.Label class="mb-1.5 block text-label-medium">
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
                <ArkField.ErrorText class="mt-1.5 text-body-small text-error font-semibold">
                  {field().state.meta.errors[0]}
                </ArkField.ErrorText>
              </Show>
              <ArkField.HelperText class="mt-1.5 text-body-small text-on-surface-variant/80">
                Name displayed in Jellyfin cast menu.
              </ArkField.HelperText>
            </ArkField.Root>
          )}
        </props.form.Field>

        <props.form.Field name="mpvPath">
          {(field) => (
            <ArkField.Root class="block">
              <ArkField.Label class="mb-1.5 block text-label-medium">
                MPV executable path
              </ArkField.Label>
              <div class="flex flex-col gap-2.5 sm:flex-row">
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
                  class="btn-secondary min-h-14 sm:min-h-0"
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
          <Collapsible.Trigger class="btn-text px-0 group cursor-pointer font-bold flex items-center gap-1 hover:text-secondary">
            <Collapsible.Indicator>
              <ChevronDown
                class={`h-4.5 w-4.5 transition-transform duration-300 ${ui.advancedOpen ? 'rotate-180 text-secondary' : 'text-on-surface-variant'}`}
              />
            </Collapsible.Indicator>
            <span>Advanced MPV options</span>
          </Collapsible.Trigger>

          <Collapsible.Content class="rounded-2xl border border-outline-variant bg-surface-container-lowest/30 p-4 mt-3 backdrop-blur-sm">
            <section class="space-y-4">
              <div>
                <h3 class="text-title-small text-on-surface flex items-center gap-2">
                  <span class="w-1 h-3 rounded bg-secondary" />
                  MPV arguments
                </h3>
                <p class="mt-1 text-body-small text-on-surface-variant/70">
                  Extra command-line flags passed to the external MPV process.
                </p>
              </div>

              <props.form.Field name="mpvArgs">
                {(field) => (
                  <ArkField.Root class="block">
                    <ArkField.Label class="mb-1.5 block text-label-medium">
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
                      class="input-filled h-auto w-full py-3.5 font-mono text-body-small"
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
          class="rounded-2xl bg-surface-container-high/30 p-5 border border-outline-variant/60 relative overflow-hidden backdrop-blur-sm shadow-inner"
        >
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div>
              <h3 class="text-title-medium text-on-surface flex items-center gap-2">
                <Globe class="h-4.5 w-4.5 text-secondary" />
                Preferred subtitle languages
              </h3>
              <p class="mt-1 text-body-small text-on-surface-variant/80">
                Add Jellyfin language codes in fallback priority order.
              </p>
            </div>
            <Show when={ui.selectedSubtitleLanguages.length > 0}>
              <button
                type="button"
                class="btn-text min-w-0 px-3 py-1 font-bold text-[13px] border border-outline-variant hover:border-secondary hover:bg-secondary/5 rounded-xl"
                onClick={props.onClearSubtitleLanguages}
              >
                Clear all
              </button>
              <TagsInput.ClearTrigger class="hidden" />
            </Show>
          </div>

          <div class="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-2">
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
              <Select.Label class="mb-1.5 block text-label-medium">
                Predefined languages
              </Select.Label>
              <Select.Control class="select-filled flex w-full items-center">
                <Select.Trigger class="flex h-14 w-full items-center justify-between gap-2 rounded-2xl border border-outline-variant/80 bg-surface-container-highest/30 px-4 text-on-surface outline-none transition-all duration-200 hover:border-secondary/50 focus:border-secondary focus:ring-4 focus:ring-secondary/15">
                  <Select.ValueText
                    placeholder="Select a language…"
                    class="text-on-surface-variant/60 font-medium text-body-medium"
                  />
                  <Select.Indicator>
                    <ChevronDown class="h-4 w-4 text-on-surface-variant/70" />
                  </Select.Indicator>
                </Select.Trigger>
              </Select.Control>
              <Select.Positioner>
                <Select.Content class="mt-2 rounded-2xl border border-outline-variant bg-surface-container-lowest p-2 shadow-2xl backdrop-blur-md max-h-60 overflow-y-auto z-50">
                  <For each={props.subtitleLanguageSelectCollection.items}>
                    {(item) => (
                      <Select.Item
                        item={item}
                        class="flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-body-medium text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface transition-colors"
                      >
                        <Select.ItemText class="font-medium">
                          {item.label}
                        </Select.ItemText>
                      </Select.Item>
                    )}
                  </For>
                </Select.Content>
              </Select.Positioner>
              <Select.HiddenSelect />
            </Select.Root>

            <div class="flex min-w-0 flex-col">
              <label
                for="custom-subtitle-lang-input"
                class="mb-1.5 block text-label-medium"
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
                  class="inline-flex h-14 min-w-[5.5rem] items-center justify-center rounded-2xl bg-secondary-container/40 border border-secondary/20 hover:border-secondary/40 px-4 text-[14px] leading-[20px] font-bold text-on-secondary-container transition duration-200 hover:bg-secondary-container/60 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50"
                  disabled={
                    parseSubtitleLanguageInput(ui.subtitleLanguageInput)
                      .length === 0
                  }
                  onClick={props.onAddSubtitleLanguages}
                >
                  <Plus class="h-4 w-4 mr-1" />
                  <span>Add</span>
                </button>
              </div>
            </div>
          </div>

          <Show
            when={ui.selectedSubtitleLanguages.length > 0}
            fallback={
              <p class="mt-5 rounded-2xl border border-dashed border-outline-variant bg-surface-container-lowest/20 px-4 py-4 text-center text-body-small text-on-surface-variant/80 backdrop-blur-sm">
                No preferred subtitle languages selected. JMSR will use Jellyfin
                and media defaults.
              </p>
            }
          >
            <ol
              class="mt-5 flex flex-col gap-2 relative z-10"
              aria-label="Selected preferred subtitle languages"
            >
              <For each={ui.selectedSubtitleLanguages}>
                {(language, index) => (
                  <TagsInput.Item
                    index={index()}
                    value={language}
                    class="flex items-center justify-between gap-3 rounded-xl border border-outline-variant bg-surface-container-lowest/60 px-4 py-2.5 backdrop-blur-sm transition-all hover:bg-surface-container-lowest/80"
                  >
                    <TagsInput.ItemPreview class="flex items-center gap-3 min-w-0 flex-1">
                      <span class="flex h-6 w-6 items-center justify-center rounded-lg bg-surface-container-high/60 border border-outline-variant text-[11px] font-bold text-secondary font-mono shadow-inner shrink-0">
                        {index() + 1}
                      </span>
                      <TagsInput.ItemText class="font-mono text-[14px] font-bold text-on-surface shrink-0">
                        {language}
                      </TagsInput.ItemText>
                      <span class="text-body-small text-on-surface-variant/80 truncate">
                        {getSubtitleLanguageLabel(language)}
                      </span>
                    </TagsInput.ItemPreview>
                    <div class="flex items-center gap-1.5 shrink-0">
                      <button
                        type="button"
                        class="btn-icon h-8 w-8 rounded-lg border border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary"
                        disabled={index() === 0}
                        aria-label={`Move ${language} up`}
                        onClick={() =>
                          props.onMoveSubtitleLanguage(index(), -1)
                        }
                      >
                        <ArrowUp class="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        class="btn-icon h-8 w-8 rounded-lg border border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary"
                        disabled={
                          index() === ui.selectedSubtitleLanguages.length - 1
                        }
                        aria-label={`Move ${language} down`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), 1)}
                      >
                        <ArrowDown class="h-4 w-4" />
                      </button>
                      <TagsInput.ItemDeleteTrigger
                        class="btn-icon h-8 w-8 rounded-lg border border-outline-variant/60 bg-surface-container-high/30 hover:border-error hover:text-error"
                        aria-label={`Remove ${language}`}
                        onClick={() => props.onRemoveSubtitleLanguage(language)}
                      >
                        <Trash2 class="h-4 w-4" />
                      </TagsInput.ItemDeleteTrigger>
                    </div>
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
