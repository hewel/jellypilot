import { Collapsible } from '@ark-ui/solid/collapsible';
import { Field as ArkField } from '@ark-ui/solid/field';
import { TagsInput } from '@ark-ui/solid/tags-input';
import { cx } from '@styled-system/css';
import { ArrowDown, ArrowUp, ChevronDown, Globe, Plus, Settings, Trash2 } from 'lucide-solid';
import { For, Show } from 'solid-js';

import { Button, FieldControl, FieldTextarea, JellyPilotSelect, SectionCard } from '../ui';
import type { JellyPilotSelectItem } from '../ui';
import * as styles from './PlayerBridgeSettingsCard.styles';
import * as shared from './shared.styles';
import { useOperationsConsoleStore } from './store';
import { getSubtitleLanguageLabel, parseSubtitleLanguageInput } from './subtitleLanguages';
import type { OperationsConsoleForm } from './types';

interface PlayerBridgeSettingsCardProps {
  form: OperationsConsoleForm;
  subtitleLanguageSelectItems: JellyPilotSelectItem[];
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
      icon={<Settings class={shared.sectionIcon.primary} />}
      title="Player Bridge settings"
      trailing={
        <Show when={ui.playerBridgeSaveStatus}>
          {(status) => (
            <span
              class={styles.saveBadge}
              classList={{
                [styles.saveError]: status().type === 'error',
                [styles.saveOk]: status().type !== 'error',
              }}
            >
              {status().text}
            </span>
          )}
        </Show>
      }
    >
      <div class={shared.stack4}>
        <props.form.Field
          name="deviceName"
          validators={{
            onBlur: ({ value }) => (!value.trim() ? 'Device name is required' : undefined),
          }}
        >
          {(field) => (
            <ArkField.Root class={styles.field} invalid={field().state.meta.errors.length > 0}>
              <ArkField.Label class={shared.overline}>Playback Target name</ArkField.Label>
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
                    class={styles.fullWidth}
                    placeholder="JellyPilot"
                  />
                )}
              />
              <Show when={field().state.meta.errors.length > 0}>
                <ArkField.ErrorText class={styles.error}>
                  {field().state.meta.errors[0]}
                </ArkField.ErrorText>
              </Show>
              <ArkField.HelperText class={styles.helper}>
                Name displayed in Jellyfin cast menu.
              </ArkField.HelperText>
            </ArkField.Root>
          )}
        </props.form.Field>

        <props.form.Field name="mpvPath">
          {(field) => (
            <ArkField.Root class={styles.field}>
              <ArkField.Label class={shared.overline}>MPV executable path</ArkField.Label>
              <div class={styles.detectRow}>
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
                      class={styles.flexInput}
                    />
                  )}
                />
                <Button
                  type="button"
                  onClick={props.onDetectMpv}
                  disabled={ui.detectingMpv}
                  variant="secondary"
                  class={styles.detectButton}
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
                class={styles.advancedTrigger}
              />
            )}
          >
            <Collapsible.Indicator>
              <ChevronDown
                class={styles.chevron}
                classList={{ [styles.chevronOpen]: ui.advancedOpen }}
              />
            </Collapsible.Indicator>
            <span>Advanced MPV options</span>
          </Collapsible.Trigger>

          <Collapsible.Content class={styles.advancedPanel}>
            <section class={shared.stack4}>
              <div>
                <h3 class={styles.subheading}>
                  <span class={styles.subheadingAccent} />
                  MPV arguments
                </h3>
                <p class={styles.helper}>
                  Extra command-line flags passed to the external MPV process.
                </p>
              </div>

              <props.form.Field name="mpvArgs">
                {(field) => (
                  <ArkField.Root class={styles.field}>
                    <ArkField.Label class={shared.overline}>Extra arguments</ArkField.Label>
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
                          class={styles.textarea}
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
          class={styles.languagePanel}
        >
          <div class={styles.panelHeader}>
            <div>
              <h3 class={styles.languageTitle}>
                <Globe class={styles.languageIcon} />
                Preferred subtitle languages
              </h3>
              <p class={styles.helper}>Add Jellyfin language codes in fallback priority order.</p>
            </div>
            <Show when={ui.selectedSubtitleLanguages.length > 0}>
              <Button
                type="button"
                variant="text"
                size="sm"
                class={styles.clearButton}
                onClick={props.onClearSubtitleLanguages}
              >
                Clear all
              </Button>
              <TagsInput.ClearTrigger class={styles.hidden} />
            </Show>
          </div>

          <div class={styles.languageGrid}>
            <JellyPilotSelect
              label="Predefined languages"
              items={props.subtitleLanguageSelectItems}
              value={null}
              placeholder="Select a language…"
              onValueChange={(value) => {
                props.onAddSubtitleLanguageCodes([value]);
              }}
            />

            <ArkField.Root class={styles.customField}>
              <ArkField.Label class={shared.overline}>Custom code</ArkField.Label>
              <div class={styles.addRow}>
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
                      class={cx(styles.flexInput, styles.mono)}
                      placeholder="e.g. pol, tha"
                      aria-label="Custom subtitle language code"
                    />
                  )}
                />
                <button
                  type="button"
                  class={styles.addButton}
                  disabled={parseSubtitleLanguageInput(ui.subtitleLanguageInput).length === 0}
                  onClick={props.onAddSubtitleLanguages}
                >
                  <Plus class={styles.plusIcon} />
                  <span>Add</span>
                </button>
              </div>
            </ArkField.Root>
          </div>

          <Show
            when={ui.selectedSubtitleLanguages.length > 0}
            fallback={
              <p class={styles.empty}>
                No preferred subtitle languages selected. JellyPilot will use Jellyfin and media
                defaults.
              </p>
            }
          >
            <ol class={styles.list} aria-label="Selected preferred subtitle languages">
              <For each={ui.selectedSubtitleLanguages}>
                {(language, index) => (
                  <TagsInput.Item index={index()} value={language} class={styles.item}>
                    <TagsInput.ItemPreview class={styles.itemPreview}>
                      <span class={styles.indexBadge}>{index() + 1}</span>
                      <TagsInput.ItemText class={styles.code}>{language}</TagsInput.ItemText>
                      <span class={styles.itemLabel}>{getSubtitleLanguageLabel(language)}</span>
                    </TagsInput.ItemPreview>
                    <div class={styles.itemActions}>
                      <Button
                        type="button"
                        variant="icon"
                        size="sm"
                        class={styles.smallIconButton}
                        disabled={index() === 0}
                        aria-label={`Move ${language} up`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), -1)}
                      >
                        <ArrowUp class={styles.icon4} />
                      </Button>
                      <Button
                        type="button"
                        variant="icon"
                        size="sm"
                        class={styles.smallIconButton}
                        disabled={index() === ui.selectedSubtitleLanguages.length - 1}
                        aria-label={`Move ${language} down`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), 1)}
                      >
                        <ArrowDown class={styles.icon4} />
                      </Button>
                      <TagsInput.ItemDeleteTrigger
                        asChild={(triggerProps) => (
                          <Button
                            {...triggerProps()}
                            type="button"
                            variant="icon"
                            size="sm"
                            class={cx(styles.smallIconButton, styles.deleteButton)}
                            aria-label={`Remove ${language}`}
                            onClick={() => props.onRemoveSubtitleLanguage(language)}
                          >
                            <Trash2 class={styles.icon4} />
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
