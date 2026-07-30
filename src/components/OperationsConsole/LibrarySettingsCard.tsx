import { Dialog } from '@ark-ui/solid/dialog';
import { cx } from '@styled-system/css';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit } from 'effect';
import { Check, Images, Trash2 } from 'lucide-solid';
import { Show, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';
import { clearImageCache, fetchImageCacheStatus } from '~effects/library';
import { queryKeys, runExit } from '~effects/query';
import * as recipes from '~styles/recipes';

import { useToast } from '../ToastProvider';
import { Button, SectionCard } from '../ui';
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
  const { showToast } = useToast();
  const queryClient = useQueryClient();
  const [confirmOpen, setConfirmOpen] = createSignal(false);

  // Refresh only while Library settings is visible; the query goes inactive when
  // the settings dialog unmounts, so there is no background global poll.
  const statusQuery = createQuery(() => ({
    queryKey: queryKeys.imageCacheStatus,
    queryFn: () => runExit(fetchImageCacheStatus),
    refetchInterval: 5000,
  }));
  const status = () =>
    statusQuery.data && Exit.isSuccess(statusQuery.data) ? statusQuery.data.value : undefined;
  const statusFailed = () => statusQuery.data && Exit.isFailure(statusQuery.data);

  const clearMutation = createMutation(() => ({
    mutationFn: () => runExit(clearImageCache),
    onSuccess: (exit) => {
      if (Exit.isSuccess(exit)) {
        showToast('success', 'Library image cache cleared');
      } else {
        showToast('error', 'Failed to clear the image cache');
      }
      setConfirmOpen(false);
      void queryClient.invalidateQueries({ queryKey: queryKeys.imageCacheStatus });
    },
    onError: () => {
      showToast('error', 'Failed to clear the image cache');
      setConfirmOpen(false);
    },
  }));

  const clearable = () => {
    const current = status();
    return Boolean(current && (current.committedBytes > 0 || current.pendingCount > 0));
  };

  return (
    <SectionCard icon={<Images class={shared.sectionIcon.primary} />} title="Library">
      <button
        type="button"
        role="checkbox"
        aria-label="Image disk cache"
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
          <span class={styles.title}>Image disk cache</span>
          <p class={styles.description}>
            Cache Library artwork locally for faster repeat browsing.
          </p>
          <Show when={!props.imageDiskCacheEnabled}>
            <p class={styles.description} data-testid="image-cache-disabled-copy">
              Off: artwork reads/writes and background optimization are bypassed; existing cached
              artwork is kept until you clear it.
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
                <span class={styles.statLabel}>Saved</span>
                <span class={styles.statValue} data-testid="image-cache-savings">
                  {formatBytes(current().estimatedSavings)}
                </span>
              </div>
              <div class={styles.stat}>
                <span class={styles.statLabel}>Pending</span>
                <span class={styles.statValue} data-testid="image-cache-pending">
                  {current().pendingCount}
                </span>
              </div>
              <div class={styles.stat}>
                <span class={styles.statLabel}>Failed</span>
                <span class={styles.statValue} data-testid="image-cache-failures">
                  {current().terminalFailures}
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
                disabled={!clearable() || clearMutation.isPending}
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
                    disabled={clearMutation.isPending}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    onClick={() => clearMutation.mutate()}
                    disabled={clearMutation.isPending}
                    data-testid="image-cache-clear-confirm"
                  >
                    {clearMutation.isPending ? 'Clearing…' : 'Clear cache'}
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
