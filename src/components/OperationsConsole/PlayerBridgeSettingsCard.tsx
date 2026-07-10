import {
  Badge,
  Button,
  Collapsible,
  IconButton,
  Selector,
  TextArea,
  TextInput,
} from '@jellypilot/ui';
import type { SelectorItem } from '@jellypilot/ui';
import { ArrowDown, ArrowUp, ChevronDown, Globe, Plus, Settings, Trash2 } from 'lucide-solid';
import { For, Show } from 'solid-js';

import ConsoleSection from './ConsoleSection';
import { useOperationsConsoleStore } from './store';
import { getSubtitleLanguageLabel, parseSubtitleLanguageInput } from './subtitleLanguages';
import type { OperationsConsoleForm } from './types';

import * as patterns from '../../styles/patterns.css';
import * as styles from './PlayerBridgeSettingsCard.css';
import * as shared from './shared.css';

interface PlayerBridgeSettingsCardProps {
  form: OperationsConsoleForm;
  subtitleLanguageSelectItems: SelectorItem[];
  onSaveTextSetting: (field: 'deviceName' | 'mpvPath' | 'mpvArgs', value: string) => void;
  onDetectMpv: () => void;
  onAddSubtitleLanguageCodes: (codes: string[]) => void;
  onAddSubtitleLanguages: () => void;
  onRemoveSubtitleLanguage: (language: string) => void;
  onClearSubtitleLanguages: () => void;
  onMoveSubtitleLanguage: (index: number, direction: -1 | 1) => void;
}

const saveStatusTone = {
  error: 'danger',
  saved: 'success',
  saving: 'warning',
} as const;

export default function PlayerBridgeSettingsCard(props: PlayerBridgeSettingsCardProps) {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <ConsoleSection
      icon={<Settings class={shared.sectionIcon.primary} />}
      title="Player Bridge settings"
      trailing={
        <Show when={ui.playerBridgeSaveStatus}>
          {(status) => <Badge tone={saveStatusTone[status().type]}>{status().text}</Badge>}
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
            <TextInput
              name={field().name}
              label="Playback Target name"
              description="Name displayed in Jellyfin cast menu."
              error={field().state.meta.errors[0]}
              value={field().state.value}
              onValueChange={(value) => field().handleChange(value)}
              onBlur={(event) => {
                field().handleBlur();
                props.onSaveTextSetting('deviceName', event.currentTarget.value);
              }}
              placeholder="JellyPilot"
            />
          )}
        </props.form.Field>

        <props.form.Field name="mpvPath">
          {(field) => (
            <div class={styles.field}>
              <div class={styles.detectRow}>
                <div class={styles.flexInput}>
                  <TextInput
                    name={field().name}
                    label="MPV executable path"
                    value={field().state.value}
                    onValueChange={(value) => field().handleChange(value)}
                    onBlur={(event) => {
                      field().handleBlur();
                      props.onSaveTextSetting('mpvPath', event.currentTarget.value);
                    }}
                    placeholder="Path to mpv executable"
                  />
                </div>
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
            </div>
          )}
        </props.form.Field>

        <Collapsible
          open={ui.advancedOpen}
          onOpenChange={(next) => actions.setAdvancedOpen(next)}
          trigger={
            <span class={styles.advancedTrigger}>
              <ChevronDown
                class={styles.chevron}
                classList={{ [styles.chevronOpen]: ui.advancedOpen }}
              />
              <span>Advanced MPV options</span>
            </span>
          }
        >
          <section class={styles.advancedPanel}>
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
                <TextArea
                  name={field().name}
                  label="Extra arguments"
                  value={field().state.value}
                  onValueChange={(value) => field().handleChange(value)}
                  onBlur={(event) => {
                    field().handleBlur();
                    props.onSaveTextSetting('mpvArgs', event.currentTarget.value);
                  }}
                  rows={4}
                  placeholder={'--fullscreen\n--force-window'}
                  class={styles.textarea}
                />
              )}
            </props.form.Field>
          </section>
        </Collapsible>

        <section class={styles.languagePanel} aria-label="Preferred subtitle languages">
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
                variant="ghost"
                size="sm"
                class={styles.clearButton}
                onClick={props.onClearSubtitleLanguages}
              >
                Clear all
              </Button>
            </Show>
          </div>

          <div class={styles.languageGrid}>
            <div class={styles.customField}>
              <span class={shared.overline}>Predefined languages</span>
              <Selector
                value={null}
                items={props.subtitleLanguageSelectItems}
                placeholder="Select a language…"
                aria-label="Predefined languages"
                onValueChange={(value) => {
                  if (value) props.onAddSubtitleLanguageCodes([value]);
                }}
              />
            </div>
            <div class={styles.customField}>
              <div class={styles.addRow}>
                <div class={styles.flexInput}>
                  <TextInput
                    id="custom-subtitle-lang-input"
                    label="Custom code"
                    value={ui.subtitleLanguageInput}
                    onValueChange={(value) => actions.setSubtitleLanguageInput(value)}
                    onKeyDown={(event) => {
                      if (event.key !== 'Enter') return;
                      event.preventDefault();
                      props.onAddSubtitleLanguages();
                    }}
                    placeholder="e.g. pol, tha"
                  />
                </div>
                <Button
                  type="button"
                  variant="secondary"
                  class={styles.addButton}
                  disabled={parseSubtitleLanguageInput(ui.subtitleLanguageInput).length === 0}
                  onClick={props.onAddSubtitleLanguages}
                >
                  <Plus class={styles.plusIcon} />
                  Add
                </Button>
              </div>
            </div>
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
                  <li class={styles.item}>
                    <div class={styles.itemPreview}>
                      <span class={styles.indexBadge}>{index() + 1}</span>
                      <span class={styles.code}>{language}</span>
                      <span class={styles.itemLabel}>{getSubtitleLanguageLabel(language)}</span>
                    </div>
                    <div class={styles.itemActions}>
                      <IconButton
                        variant="outline"
                        class={styles.smallIconButton}
                        disabled={index() === 0}
                        aria-label={`Move ${language} up`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), -1)}
                      >
                        <ArrowUp class={patterns.icon4} />
                      </IconButton>
                      <IconButton
                        variant="outline"
                        class={styles.smallIconButton}
                        disabled={index() === ui.selectedSubtitleLanguages.length - 1}
                        aria-label={`Move ${language} down`}
                        onClick={() => props.onMoveSubtitleLanguage(index(), 1)}
                      >
                        <ArrowDown class={patterns.icon4} />
                      </IconButton>
                      <IconButton
                        variant="outline"
                        class={`${styles.smallIconButton} ${styles.deleteButton}`}
                        aria-label={`Remove ${language}`}
                        onClick={() => props.onRemoveSubtitleLanguage(language)}
                      >
                        <Trash2 class={patterns.icon4} />
                      </IconButton>
                    </div>
                  </li>
                )}
              </For>
            </ol>
          </Show>
        </section>
      </div>
    </ConsoleSection>
  );
}
