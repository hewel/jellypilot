import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit } from 'effect';
import { clearImageCache, fetchImageCacheStatus } from '~effects/library';
import { queryKeys, runExit } from '~effects/query';

import { useToast } from '../ToastProvider';

/**
 * Operations Console controls for the Library Image cache: status polling and
 * destructive clearing. The enable toggle stays with the parent (its config
 * save-queue is the `setEnabled` adapter); this module owns everything else
 * about the cache-control interface.
 */
export function createLibraryImageCacheControls() {
  const { showToast } = useToast();
  const queryClient = useQueryClient();

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
      void queryClient.invalidateQueries({ queryKey: queryKeys.imageCacheStatus });
    },
    onError: () => {
      showToast('error', 'Failed to clear the image cache');
    },
  }));

  return {
    status,
    statusFailed,
    /** A destructive-maintenance epoch is in progress (this or another process). */
    clearing: () => status()?.clearing ?? false,
    clearable: () => {
      const current = status();
      return Boolean(current && (current.committedBytes > 0 || current.pendingCount > 0));
    },
    clearPending: () => clearMutation.isPending,
    /** Run the clear; `onSettled` fires after success or failure (e.g. to close a dialog). */
    requestClear: (onSettled?: () => void) => clearMutation.mutate(undefined, { onSettled }),
  };
}
