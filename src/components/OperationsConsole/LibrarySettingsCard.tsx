import { Dialog } from '@ark-ui/solid/dialog';
import { cx } from '@styled-system/css';
import { Check, Images, Trash2 } from 'lucide-solid';
import { Show, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';
import * as recipes from '~styles/recipes';

import { Button, SectionCard } from '../ui';
import { createLibraryImageCacheControls } from './libraryImageCacheControls';
import * as styles from './LibrarySettingsCard.styles';
import * as shared from './shared.styles';

interface LibrarySettingsCardProps {
  imageDiskCacheEnabled: boolean;
  onImageDiskCacheEnabledChange: (enabled: boolean) => void;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

export default function LibrarySettingsCard(props: LibrarySettingsCardProps) {
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const cache = createLibraryImageCacheControls();
  const status = cache.status;
  const statusFailed = cache.statusFailed;

  return (
    <SectionCard icon={<Images class={shared.sectionIcon.primary} />} title="Library">
      <button
        type="button"
        role="checkbox"
        aria-label="Library Image cache"
        aria-checked={props.imageDiskCacheEnabled}
        onClick={() => props.onImageDiskCacheEnabledChange(!props.imageDiskCacheEnabled)}
        class={styles.toggle}
      >
        <span
          aria-hidden="true"
          class={cx(recipes.checkboxBox, styles.checkboxOffset)}
          classList={{ [styles.checkboxChecked]: props.imageDiskCacheEnabled }}
        >
          <Show when={props.imageDiskCacheEnabled}>
            <Check class={styles.icon3_5} stroke-width={3} />
          </Show>
        </span>
        <div class={styles.copy}>
          <span class={styles.title}>Library Image cache</span>
          <p class={styles.description}>
            Cache Library artwork locally for faster repeat browsing.
          </p>
          <Show when={!props.imageDiskCacheEnabled}>
            <p class={styles.description} data-testid="image-cache-disabled-copy">
              Off: artwork reads and writes bypass the cache; existing cached artwork is kept until
              you clear it.
            </p>
          </Show>
        </div>
      </button>

      <div class={styles.stats} aria-live="polite">
        <Show
          when={status()}
          fallback={
            <Show
              when={!statusFailed()}
              fallback={<p class={styles.statError}>Cache status unavailable.</p>}
            >
              <p class={styles.statPlaceholder} data-testid="image-cache-status-loading">
                Loading cache status…
              </p>
            </Show>
          }
        >
          {(current) => (
            <>
              <div class={styles.stat}>
                <span class={styles.statLabel}>Cached</span>
                <span class={styles.statValue} data-testid="image-cache-usage">
                  {formatBytes(current().committedBytes)}
                </span>
              </div>
              <div class={styles.stat}>
                <span class={styles.statLabel}>Images</span>
                <span class={styles.statValue} data-testid="image-cache-count">
                  {current().entryCount}
                </span>
              </div>
            </>
          )}
        </Show>
      </div>

      <div class={styles.clearRow}>
        <Dialog.Root
          open={confirmOpen()}
          onOpenChange={(d) => setConfirmOpen(d.open)}
          lazyMount
          unmountOnExit
        >
          <Dialog.Trigger
            asChild={(triggerProps) => (
              <Button
                {...triggerProps()}
                variant="outlined"
                size="sm"
                leadingIcon={<Trash2 class={styles.icon3_5} />}
                disabled={!cache.clearable() || cache.clearPending() || cache.clearing()}
              >
                Clear cache
              </Button>
            )}
          />
          <Portal>
            <Dialog.Backdrop class={recipes.scrim({ tone: 'dark', z: '60' })} />
            <Dialog.Positioner class={recipes.modalPositioner({ align: 'center', z: '60' })}>
              <Dialog.Content class={styles.dialogContent}>
                <Dialog.Title class={styles.dialogTitle}>Clear Library image cache?</Dialog.Title>
                <Dialog.Description class={styles.dialogDescription}>
                  This removes cached artwork for every saved server. Artwork reloads from your
                  media server as you browse.
                </Dialog.Description>
                <div class={styles.dialogActions}>
                  <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setConfirmOpen(false)}
                    disabled={cache.clearPending()}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    onClick={() => cache.requestClear(() => setConfirmOpen(false))}
                    disabled={cache.clearPending()}
                    data-testid="image-cache-clear-confirm"
                  >
                    {cache.clearPending() ? 'Clearing…' : 'Clear cache'}
                  </Button>
                </div>
              </Dialog.Content>
            </Dialog.Positioner>
          </Portal>
        </Dialog.Root>
      </div>
    </SectionCard>
  );
}
