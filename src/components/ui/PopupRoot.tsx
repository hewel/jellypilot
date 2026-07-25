import { createContext, createSignal, useContext } from 'solid-js';
import type { Accessor, ParentProps } from 'solid-js';

import * as styles from './PopupRoot.styles';

const PopupRootContext = createContext<Accessor<HTMLElement | null>>();

/**
 * App-level host for floating UI (hover cards, etc.). Popups portal into the
 * mount node so they are never nested under buttons, links, or scroll clips.
 */
export function PopupRoot(props: ParentProps) {
  const [mount, setMount] = createSignal<HTMLElement | null>(null);

  return (
    <PopupRootContext.Provider value={mount}>
      {props.children}
      <div ref={setMount} class={styles.root} data-popup-root aria-hidden="true" />
    </PopupRootContext.Provider>
  );
}

/** Portal mount for popups; `null` until the root node is attached. */
export function usePopupRootMount(): Accessor<HTMLElement | null> | undefined {
  return useContext(PopupRootContext);
}
