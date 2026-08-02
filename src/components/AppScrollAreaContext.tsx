import { createContext, createSignal, useContext } from 'solid-js';
import type { Accessor, JSX, ParentProps } from 'solid-js';

export interface AppScrollAreaApi {
  viewport: Accessor<HTMLElement | null>;
  scrolled: Accessor<boolean>;
  setViewport: (el: HTMLElement | null) => void;
  handleViewportScroll: JSX.EventHandler<HTMLElement, Event>;
  scrollTo: (options: ScrollToOptions) => void;
}

const SCROLLED_THRESHOLD_PX = 4;

const AppScrollAreaContext = createContext<AppScrollAreaApi>();

export function createAppScrollAreaController(): AppScrollAreaApi {
  const [viewport, setViewportSignal] = createSignal<HTMLElement | null>(null);
  const [scrolled, setScrolled] = createSignal(false);

  const syncScrolled = (scrollTop: number | undefined) => {
    setScrolled((scrollTop ?? 0) > SCROLLED_THRESHOLD_PX);
  };

  const setViewport = (el: HTMLElement | null) => {
    setViewportSignal(el);
    syncScrolled(el?.scrollTop);
  };

  const handleViewportScroll: JSX.EventHandler<HTMLElement, Event> = (event) => {
    const currentViewport = event.currentTarget;
    if (viewport() !== currentViewport) {
      return;
    }
    syncScrolled(currentViewport.scrollTop);
  };

  return {
    viewport,
    scrolled,
    setViewport,
    handleViewportScroll,
    scrollTo: (options) => {
      const currentViewport = viewport();
      currentViewport?.scrollTo(options);
      syncScrolled(currentViewport?.scrollTop);
    },
  };
}

export function AppScrollAreaProvider(props: ParentProps<{ value: AppScrollAreaApi }>) {
  return (
    <AppScrollAreaContext.Provider value={props.value}>
      {props.children}
    </AppScrollAreaContext.Provider>
  );
}

export function useAppScrollArea(): AppScrollAreaApi {
  const context = useContext(AppScrollAreaContext);

  if (!context) {
    throw new Error('App scroll area is only available under the root route');
  }

  return context;
}
