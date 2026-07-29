import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { LibrarySearchResultRow } from '@components/library/LibrarySearchResultRow';
import { LibraryStatusPanel } from '@components/library/shared';
import { Button } from '@components/ui';
import { cx } from '@styled-system/css';
import { createInfiniteQuery, createQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, redirect } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { RefreshCw } from 'lucide-solid';
import { For, Show, createEffect, createMemo, createSignal, onCleanup } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import { fetchVideoSearchPage } from '~effects/library';
import type { LibraryExit, LibrarySearchState } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';
import * as recipes from '~styles/recipes';

import * as styles from './search.styles';

interface LibrarySearchInfiniteData {
  pages: LibraryExit<LibrarySearchState>[];
  pageParams: number[];
}

export const Route = createFileRoute('/_authenticated/library/search')({
  validateSearch: (search: Record<string, unknown>): { q: string } => ({
    q: typeof search.q === 'string' ? search.q.trim() : '',
  }),
  beforeLoad: ({ search }) => {
    if (search.q === '') {
      throw redirect({ replace: true, to: '/library' });
    }
  },
  component: LibrarySearchRoute,
});

function LibrarySearchRoute() {
  const search = Route.useSearch();
  const q = () => search().q;
  const queryClient = useQueryClient();
  const appScroll = useAppScrollArea();
  const [autoLoadSentinel, setAutoLoadSentinel] = createSignal<HTMLDivElement | null>(null);
  const [autoLoadSentinelVisible, setAutoLoadSentinelVisible] = createSignal(false);
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const connected = () => isLibrarySessionKeyConnected(sessionKey());

  const searchQueryKey = () => queryKeys.librarySearch(sessionKey(), q());
  const searchQuery = createInfiniteQuery(() => ({
    queryKey: searchQueryKey(),
    enabled: connected() && q() !== '',
    queryFn: ({ pageParam }) => {
      const startIndex = typeof pageParam === 'number' ? pageParam : 0;
      return runExit(fetchVideoSearchPage(q(), startIndex));
    },
    initialPageParam: 0,
    getNextPageParam: (lastPage) =>
      Exit.match(lastPage, {
        onFailure: () => undefined,
        onSuccess: (page) => (page.hasMore ? page.startIndex + page.limit : undefined),
      }),
  }));

  const pages = () => searchQuery.data?.pages ?? [];
  const firstPage = () => pages()[0] ?? null;
  const firstPageState = () => {
    const current = firstPage();
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const items = () =>
    pages()
      .filter((page): page is LibraryExit<LibrarySearchState> & { _tag: 'Success' } =>
        Exit.isSuccess(page),
      )
      .flatMap((page) => page.value.items);
  const laterPageFailure = () => {
    const index = pages().findIndex((page, pageIndex) => pageIndex > 0 && !Exit.isSuccess(page));
    if (index === -1) {
      return null;
    }
    const page = pages()[index];
    return page && !Exit.isSuccess(page) ? { index, page } : null;
  };

  // A new query gets a fresh cache key, which drops any later-page failure;
  // Scroll back to the top so the new first page reads from the start.
  const searchQuerySignature = createMemo(() => JSON.stringify(searchQueryKey()));
  let activeSearchQuerySignature = '';
  createEffect(() => {
    const nextSignature = searchQuerySignature();
    if (activeSearchQuerySignature && activeSearchQuerySignature !== nextSignature) {
      appScroll.scrollTo({ top: 0 });
    }
    activeSearchQuerySignature = nextSignature;
  });

  createEffect(() => {
    const sentinel = autoLoadSentinel();
    if (!sentinel || typeof IntersectionObserver === 'undefined') {
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        setAutoLoadSentinelVisible(entries.some((entry) => entry.isIntersecting));
      },
      {
        root: null,
        rootMargin: '400px 0px',
        threshold: 0,
      },
    );
    observer.observe(sentinel);
    onCleanup(() => observer.disconnect());
  });

  createEffect(() => {
    if (!autoLoadSentinelVisible()) {
      return;
    }
    if (!searchQuery.hasNextPage || searchQuery.isFetching || laterPageFailure()) {
      return;
    }
    void searchQuery.fetchNextPage({ cancelRefetch: false });
  });

  const retryFailedPage = () => {
    const failure = laterPageFailure();
    if (!failure || searchQuery.isFetching) {
      return;
    }
    queryClient.setQueryData<LibrarySearchInfiniteData>(searchQueryKey(), (data) => {
      if (!data) {
        return data;
      }
      return {
        pages: data.pages.filter((_, index) => index !== failure.index),
        pageParams: data.pageParams.filter((_, index) => index !== failure.index),
      };
    });
    void searchQuery.fetchNextPage({ cancelRefetch: false });
  };

  const statusState = () => {
    if (!connected()) {
      return {
        title: 'Connect to search',
        description:
          'JellyPilot needs an active Jellyfin or Emby connection to search every video library.',
        retry: false,
      };
    }
    const current = firstPage();
    if (current && !Exit.isSuccess(current)) {
      return {
        title: 'Search failed',
        description: commandFailureMessage(current.cause, 'Could not search the library'),
        retry: true,
      };
    }
    return {
      title: 'Searching library',
      description: `JellyPilot is searching every video library for “${q()}”.`,
      retry: false,
    };
  };

  return (
    <div class={cx(styles.root, styles.pageGutter)}>
      <header class={styles.header}>
        <h1 class={styles.heading}>Search results for “{q()}”</h1>
        <Show when={firstPageState()}>
          {(page) => <p class={styles.count}>{page().totalRecordCount} results</p>}
        </Show>
      </header>
      <Show
        when={firstPageState()}
        fallback={
          <>
            <LibraryStatusPanel
              title={statusState().title}
              description={statusState().description}
            />
            <Show when={statusState().retry}>
              <div class={styles.statusActions}>
                <Button
                  type="button"
                  variant="secondary"
                  class={recipes.pillButton}
                  disabled={searchQuery.isFetching}
                  onClick={() => void searchQuery.refetch()}
                  leadingIcon={
                    <RefreshCw
                      class={styles.icon4}
                      classList={{ [styles.spin]: searchQuery.isFetching }}
                    />
                  }
                >
                  Retry search
                </Button>
              </div>
            </Show>
          </>
        }
      >
        {(page) => (
          <Show
            when={page().items.length > 0}
            fallback={
              <LibraryStatusPanel
                title={`No results for “${q()}”`}
                description="Try a different title, person, or episode name."
              />
            }
          >
            <section class={styles.root} aria-label={`Search results for ${q()}`}>
              <ul class={styles.results}>
                <For each={items()}>
                  {(item) => (
                    <li class={styles.resultItem}>
                      <LibrarySearchResultRow item={item} />
                    </li>
                  )}
                </For>
              </ul>
              <Show when={laterPageFailure()}>
                {(failure) => (
                  <div class={styles.loadMoreError}>
                    <p class={styles.error}>
                      {commandFailureMessage(failure().page.cause, 'Could not load more results')}
                    </p>
                    <Button
                      type="button"
                      variant="secondary"
                      class={recipes.pillButton}
                      disabled={searchQuery.isFetchingNextPage}
                      onClick={retryFailedPage}
                      leadingIcon={
                        <RefreshCw
                          class={styles.icon4}
                          classList={{ [styles.spin]: searchQuery.isFetchingNextPage }}
                        />
                      }
                    >
                      Retry loading more
                    </Button>
                  </div>
                )}
              </Show>
              <div aria-live="polite" class={styles.liveStatus}>
                <Show when={searchQuery.isFetchingNextPage}>Loading more results</Show>
              </div>
              <div ref={setAutoLoadSentinel} aria-hidden="true" class={styles.sentinel} />
            </section>
          </Show>
        )}
      </Show>
    </div>
  );
}
