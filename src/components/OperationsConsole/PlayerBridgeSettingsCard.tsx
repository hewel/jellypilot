import { Collapsible } from '@ark-ui/solid/collapsible';
import { Field as ArkField } from '@ark-ui/solid/field';
import { TagsInput } from '@ark-ui/solid/tags-input';
import { ArrowDown, ArrowUp, ChevronDown, Globe, Plus, Settings, Trash2 } from 'lucide-solid';
import { For, Show } from 'solid-js';

import { Button, FieldControl, FieldTextarea, JmsrSelect, SectionCard } from '../ui';
import type { JmsrSelectItem } from '../ui';
import { useOperationsConsoleStore } from './store';
import { getSubtitleLanguageLabel, parseSubtitleLanguageInput } from './subtitleLanguages';
import type { OperationsConsoleForm } from './types';

interface PlayerBridgeSettingsCardProps {
  form: OperationsConsoleForm;
  subtitleLanguageSelectItems: JmsrSelectItem[];
  onSaveTextSetting: (field: 'deviceName' | 'mpvPath' | 'mpvArgs', value: string) => void;
  onDetectMpv: () => void;
  onAddSubtitleLanguageCodes: (codes: string[]) => void;
  onAddSubtitleLanguages: () => void;
  onRemoveSubtitleLanguage: (language: string) => void;
  onClearSubtitleLanguages: () => void;
  onMoveSubtitleLanguage: (index: number, direction: -1 | 1) => void;
}

export default function PlayerBridgeSettingsCard(props: PlayerBridgeSettingsCardProps) {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <SectionCard
      icon={<Settings class="text-primary h-5 w-5 drop-shadow-[0_0_8px_rgba(79,70,229,0.4)]" />}
      title="Player Bridge settings"
      trailing={
        <Show when={ui.playerBridgeSaveStatus}>
          {(status) => (
            <span
              class={`rounded border px-2.5 py-0.5 text-[11px] font-bold tracking-wider uppercase ${
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
            onBlur: ({ value }) => (!value.trim() ? 'Device name is required' : undefined),
          }}
        >
          {(field) => (
            <ArkField.Root class="block" invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                Playback Target name
              </ArkField.Label>
              <ArkField.Input
                asChild={(fieldProps) => (
                  <FieldControl
                    {...fieldProps()}
                    variant="filled"
                    name={field().name}
                    type="text"
                    value={field().state.value}
                    onInput={(event) => field().handleChange(event.currentTarget.value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('deviceName', event.currentTarget.value);
                    }}
                    class="w-full"
                    placeholder="JMSR"
                  />
                )}
              />
              <Show when={field().state.meta.errors.length > 0}>
                <ArkField.ErrorText class="text-error mt-1.5 text-[12px] leading-[16px] font-semibold">
                  {field().state.meta.errors[0]}
                </ArkField.ErrorText>
              </Show>
              <ArkField.HelperText class="text-on-surface-variant/80 mt-1.5 text-[12px] leading-[16px]">
                Name displayed in Jellyfin cast menu.
              </ArkField.HelperText>
            </ArkField.Root>
          )}
        </props.form.Field>

        <props.form.Field name="mpvPath">
          {(field) => (
            <ArkField.Root class="block">
              <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                MPV executable path
              </ArkField.Label>
              <div class="flex flex-col gap-2.5 sm:flex-row">
                <ArkField.Input
                  asChild={(fieldProps) => (
                    <FieldControl
                      {...fieldProps()}
                      variant="filled"
                      name={field().name}
                      type="text"
                      value={field().state.value}
                      onInput={(event) => field().handleChange(event.currentTarget.value)}
                      onBlur={(event) => {
                        field().handleBlur();
                        props.onSaveTextSetting('mpvPath', event.currentTarget.value);
                      }}
                      placeholder="Path to mpv executable"
                      class="min-w-0 flex-1"
                    />
                  )}
                />
                <Button
                  type="button"
                  onClick={props.onDetectMpv}
                  disabled={ui.detectingMpv}
                  variant="secondary"
                  class="min-h-14 sm:min-h-0"
                >
                  {ui.detectingMpv ? 'Detecting...' : 'Detect MPV'}
                </Button>
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
          <Collapsible.Trigger
            asChild={(triggerProps) => (
              <Button
                {...triggerProps()}
                type="button"
                variant="text"
                class="group hover:text-secondary px-0 font-bold"
              />
            )}
          >
            <Collapsible.Indicator>
              <ChevronDown
                class={`h-4.5 w-4.5 transition-transform duration-300 ${ui.advancedOpen ? 'text-secondary rotate-180' : 'text-on-surface-variant'}`}
              />
            </Collapsible.Indicator>
            <span>Advanced MPV options</span>
          </Collapsible.Trigger>

          <Collapsible.Content class="border-outline-variant bg-surface-container-lowest/30 mt-3 rounded-2xl border p-4 backdrop-blur-sm">
            <section class="space-y-4">
              <div>
                <h3 class="text-on-surface flex items-center gap-2 text-[14px] leading-[20px] font-semibold">
                  <span class="bg-secondary h-3 w-1 rounded" />
                  MPV arguments
                </h3>
                <p class="text-on-surface-variant/70 mt-1 text-[12px] leading-[16px]">
                  Extra command-line flags passed to the external MPV process.
                </p>
              </div>

              <props.form.Field name="mpvArgs">
                {(field) => (
                  <ArkField.Root class="block">
                    <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                      Extra arguments
                    </ArkField.Label>
                    <ArkField.Textarea
                      asChild={(fieldProps) => (
                        <FieldTextarea
                          {...fieldProps()}
                          variant="filled"
                          value={field().state.value}
                          onInput={(event) => field().handleChange(event.currentTarget.value)}
                          onBlur={(event) => {
                            field().handleBlur();
                            props.onSaveTextSetting('mpvArgs', event.currentTarget.value);
                          }}
                          rows={4}
                          placeholder="--fullscreen&#10;--force-window"
                          class="text-on-surface-variant/80 h-auto w-full py-3.5 font-mono text-[12px] leading-[16px]"
                        />
                      )}
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
          class="bg-surface-container-high/30 border-outline-variant/60 relative rounded-2xl border p-5 shadow-inner backdrop-blur-sm"
        >
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div>
              <h3 class="text-on-surface flex items-center gap-2 text-[16px] leading-[24px] font-semibold">
                <Globe class="text-secondary h-4.5 w-4.5" />
                Preferred subtitle languages
              </h3>
              <p class="text-on-surface-variant/80 mt-1 text-[12px] leading-[16px]">
                Add Jellyfin language codes in fallback priority order.
              </p>
            </div>
            <Show when={ui.selectedSubtitleLanguages.length > 0}>
              <Button
                type="button"
                variant="text"
                size="sm"
                class="border-outline-variant hover:border-secondary hover:bg-secondary/5 min-w-0 rounded-xl border px-3 py-1 text-[13px] font-bold"
                onClick={props.onClearSubtitleLanguages}
              >
                Clear all
              </Button>
              <TagsInput.ClearTrigger class="hidden" />
            </Show>
          </div>

          <div class="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-2">
            <JmsrSelect
              label="Predefined languages"
              items={props.subtitleLanguageSelectItems}
              value={null}
              placeholder="Select a language…"
              onValueChange={(value) => {
                props.onAddSubtitleLanguageCodes([value]);
              }}
            />

            <ArkField.Root class="flex min-w-0 flex-col">
              <ArkField.Label class="text-on-surface-variant mb-1.5 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                Custom code
              </ArkField.Label>
              <div class="flex gap-2">
                <ArkField.Input
                  asChild={(fieldProps) => (
                    <FieldControl
                      {...fieldProps()}
                      variant="filled"
                      id="custom-subtitle-lang-input"
                      type="text"
                      value={ui.subtitleLanguageInput}
                      onInput={(event) =>
                        actions.setSubtitleLanguageInput(event.currentTarget.value)
                      }
                      onKeyDown={(event) => {
                        if (event.key !== 'Enter') {
                          return;
                        }
                        event.preventDefault();
                        props.onAddSubtitleLanguages();
                      }}
                      class="min-w-0 flex-1 font-mono"
                      placeholder="e.g. pol, tha"
                      aria-label="Custom subtitle language code"
                    />
                  )}
                />
                <button
                  type="button"
                  class="bg-secondary-container/40 border-secondary/20 hover:border-secondary/40 text-on-secondary-container hover:bg-secondary-container/60 inline-flex h-14 min-w-[5.5rem] items-center justify-center rounded-2xl border px-4 text-[14px] leading-[20px] font-bold transition duration-200 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50"
                  disabled={parseSubtitleLanguageInput(ui.subtitleLanguageInput).length === 0}
                  onClick={props.onAddSubtitleLanguages}
                >
                  <Plus class="mr-1 h-4 w-4" />
                  <span>Add</span>
                </button>
              </div>
            </ArkField.Root>
          </div>

          <Show
            when={ui.selectedSubtitleLanguages.length > 0}
            fallback={
              <p class="border-outline-variant bg-surface-container-lowest/20 text-on-surface-variant/80 mt-5 rounded-2xl border border-dashed px-4 py-4 text-center text-[12px] leading-[16px] backdrop-blur-sm">
                No preferred subtitle languages selected. JMSR will use Jellyfin and media defaults.
              </p>
            }
          >
            <ol
              class="relative z-10 mt-5 flex flex-col gap-2"
              aria-label="Selected preferred subtitle languages"
            >
              <For each={ui.selectedSubtitleLanguages}>
                {(language, index) => (
                  <TagsInput.Item
                    index={index()}
                    value={language}
                    class="border-outline-variant bg-surface-container-lowest/60 hover:bg-surface-container-lowest/80 flex items-center justify-between gap-3 rounded-xl border px-4 py-2.5 backdrop-blur-sm transition-all"
                  >
                    <TagsInput.ItemPreview class="flex min-w-0 flex-1 items-center gap-3">
                      <span class="bg-surface-container-high/60 border-outline-variant text-secondary flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border font-mono text-[11px] font-bold shadow-inner">
                        {index() + 1}
                      </span>
                      <TagsInput.ItemText class="text-on-surface shrink-0 font-mono text-[14px] font-bold">
                        {language}
                      </TagsInput.ItemText>
                      <span class="text-on-surface-variant/80 truncate text-[12px] leading-[16px]">
                        {getSubtitleLanguageLabel(language)}
                      </span>
                    </TagsInput.ItemPreview>
                    <div class="flex shrink-0 items-center gap-1.5">
                      <Button
                        type="button"
                        variant="icon"
                        size="sm"
                        class="border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary h-8 w-8 rounded-lg border"
                        disabled={index() === 0}
                        aria-label={`Move ${language} up`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), -1)}
                      >
                        <ArrowUp class="h-4 w-4" />
                      </Button>
                      <Button
                        type="button"
                        variant="icon"
                        size="sm"
                        class="border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary h-8 w-8 rounded-lg border"
                        disabled={index() === ui.selectedSubtitleLanguages.length - 1}
                        aria-label={`Move ${language} down`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), 1)}
                      >
                        <ArrowDown class="h-4 w-4" />
                      </Button>
                      <TagsInput.ItemDeleteTrigger
                        asChild={(triggerProps) => (
                          <Button
                            {...triggerProps()}
                            type="button"
                            variant="icon"
                            size="sm"
                            class="border-outline-variant/60 bg-surface-container-high/30 hover:border-error hover:text-error h-8 w-8 rounded-lg border"
                            aria-label={`Remove ${language}`}
                            onClick={() => props.onRemoveSubtitleLanguage(language)}
                          >
                            <Trash2 class="h-4 w-4" />
                          </Button>
                        )}
                      />
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
