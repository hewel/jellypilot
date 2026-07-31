import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { useQueryClient } from '@tanstack/solid-query';
import { useLocation, useNavigate } from '@tanstack/solid-router';
import { ArrowRight, Search, X } from 'lucide-solid';
import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import type { JSX } from 'solid-js';
import { isLibrarySessionKeyConnected, queryKeys } from '~effects/query';
import type { LibrarySessionKey } from '~effects/query';

import * as styles from './LibrarySearchBar.styles';
import { Button, FieldControl } from './ui';

export interface LibrarySearchBarProps {
  sessionKey: LibrarySessionKey;
  collapsed: boolean;
  onRequestExpand: () => void;
  onRequestCollapse: () => void;
}

const INTERACTIVE_TARGET_SELECTOR = 'input, textarea, select, button, a, [contenteditable]';

function isInteractiveTarget(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(INTERACTIVE_TARGET_SELECTOR) !== null;
}

/**
 * Persistent sidebar search control. Keeps one retained draft for the mounted
 * authenticated shell, navigates to `/library/search?q=…` on submit, resets
 * the active query on repeat submit, and expands/focuses on unmodified `/`.
 * Collapsed it renders a single Search trigger; expansion and collapse are
 * owned by the sidebar through `onRequestExpand`/`onRequestCollapse`.
 */
export default function LibrarySearchBar(props: LibrarySearchBarProps): JSX.Element {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const appScroll = useAppScrollArea();
  const [draft, setDraft] = createSignal('');
  const [inputElement, setInputElement] = createSignal<HTMLInputElement | null>(null);
  const [focusOnExpand, setFocusOnExpand] = createSignal(false);

  const connected = () => isLibrarySessionKeyConnected(props.sessionKey);
  const canSubmit = () => connected() && draft().trim() !== '';
  const routeQuery = useLocation({
    select: (location) => {
      if (location.pathname !== '/library/search') {
        return null;
      }
      const q = (location.search as Record<string, unknown>).q;
      return typeof q === 'string' ? q : null;
    },
  });

  // Resynchronize the draft whenever history activates the search route; on
  // every other child route the last submitted/draft value is retained.
  createEffect(() => {
    const q = routeQuery();
    if (q !== null) {
      setDraft(q);
    }
  });

  const focusInput = () => {
    const input = inputElement();
    if (!input || input.disabled) {
      return false;
    }
    input.focus();
    input.select();
    return true;
  };

  const requestExpand = () => {
    setFocusOnExpand(true);
    props.onRequestExpand();
  };

  // Focus and select only after the expanded form has mounted its input.
  createEffect(() => {
    if (!props.collapsed && focusOnExpand() && focusInput()) {
      setFocusOnExpand(false);
    }
  });

  onMount(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      if (event.key !== '/' || event.ctrlKey || event.metaKey || event.altKey) {
        return;
      }
      if (!connected() || isInteractiveTarget(event.target)) {
        return;
      }
      event.preventDefault();
      if (props.collapsed) {
        requestExpand();
        return;
      }
      focusInput();
    };
    document.addEventListener('keydown', handleKeydown);
    onCleanup(() => document.removeEventListener('keydown', handleKeydown));
  });

  const handleSubmit: JSX.EventHandler<HTMLFormElement, SubmitEvent> = (event) => {
    event.preventDefault();
    if (!canSubmit()) {
      return;
    }
    const trimmedQuery = draft().trim();
    if (routeQuery() === trimmedQuery) {
      appScroll.scrollTo({ top: 0 });
      void queryClient.resetQueries({
        exact: true,
        queryKey: queryKeys.librarySearch(props.sessionKey, trimmedQuery),
      });
      return;
    }
    inputElement()?.blur();
    void navigate({ search: { q: trimmedQuery }, to: '/library/search' });
  };

  const handleClear = () => {
    setDraft('');
    inputElement()?.focus();
  };

  const handleKeyDown: JSX.EventHandler<HTMLInputElement, KeyboardEvent> = (event) => {
    if (event.key === 'Escape' && draft().trim() === '') {
      props.onRequestCollapse();
    }
  };

  return (
    <Show
      when={!props.collapsed}
      fallback={
        <Button
          aria-label="Search library"
          class={styles.collapsedTrigger}
          onClick={requestExpand}
          size="row"
          type="button"
          variant="icon"
        >
          <Search aria-hidden="true" class={styles.collapsedTriggerIcon} />
        </Button>
      }
    >
      <form aria-label="Search library" class={styles.form} onSubmit={handleSubmit} role="search">
        <div class={styles.control}>
          <Search aria-hidden="true" class={styles.leadingIcon} size={16} />
          <FieldControl
            ref={setInputElement}
            aria-label="Search library"
            class={styles.input}
            disabled={!connected()}
            onInput={(event) => setDraft(event.currentTarget.value)}
            onKeyDown={handleKeyDown}
            placeholder={connected() ? 'Search movies, shows, and episodes' : 'Connect to search'}
            type="search"
            value={draft()}
          />
          <div class={styles.trailingAffordance}>
            <Show
              when={draft().length > 0}
              fallback={
                <span aria-hidden="true" class={styles.keycap}>
                  /
                </span>
              }
            >
              <Button
                aria-label="Clear search"
                class={styles.clearButton}
                disabled={!connected()}
                onClick={handleClear}
                size="sm"
                type="button"
                variant="icon"
              >
                <X aria-hidden="true" size={16} />
              </Button>
            </Show>
          </div>
          <Button
            aria-label="Search library"
            class={styles.submitButton}
            disabled={!canSubmit()}
            size="sm"
            type="submit"
            variant="secondary"
          >
            <ArrowRight aria-hidden="true" size={16} />
          </Button>
        </div>
      </form>
    </Show>
  );
}
