import type { Accessor, ParentProps } from 'solid-js';
import { createContext, createEffect, createSignal, onCleanup, useContext } from 'solid-js';

import { uiInvariant } from './invariant';

interface LayerContextValue {
  portalHost: Accessor<HTMLElement | null>;
  register: (id: symbol, modal: boolean) => void;
  unregister: (id: symbol) => void;
  isTopmost: (id: symbol) => boolean;
}

const LayerContext = createContext<LayerContextValue>();

export function LayerProvider(props: ParentProps<{ portalHost: Accessor<HTMLElement | null> }>) {
  const [stack, setStack] = createSignal<{ id: symbol; modal: boolean }[]>([]);
  const value: LayerContextValue = {
    portalHost: props.portalHost,
    register: (id, modal) =>
      setStack((current) => [...current.filter((entry) => entry.id !== id), { id, modal }]),
    unregister: (id) => setStack((current) => current.filter((entry) => entry.id !== id)),
    isTopmost: (id) => stack().at(-1)?.id === id,
  };

  createEffect(() => {
    if (!stack().some((entry) => entry.modal)) return;
    const host = props.portalHost();
    const ownerDocument = host?.ownerDocument;
    const applicationRoot = ownerDocument?.querySelector<HTMLElement>('[data-jp-uiroot]');
    if (!ownerDocument || !applicationRoot) return;
    const previousOverflow = ownerDocument.body.style.overflow;
    const wasInert = applicationRoot.hasAttribute('inert');
    ownerDocument.body.style.overflow = 'hidden';
    applicationRoot.setAttribute('inert', '');
    onCleanup(() => {
      ownerDocument.body.style.overflow = previousOverflow;
      if (!wasInert) applicationRoot.removeAttribute('inert');
    });
  });

  return <LayerContext.Provider value={value}>{props.children}</LayerContext.Provider>;
}

export function createLayerRegistration(options: { modal?: boolean } = {}) {
  const context = useContext(LayerContext);
  uiInvariant(
    context !== undefined,
    'layer-without-uiroot',
    'Layer components must be rendered inside UIRoot',
  );
  const id = Symbol('ui-layer');

  return {
    portalHost: context!.portalHost,
    isTopmost: () => context!.isTopmost(id),
    mount: () => {
      context!.register(id, options.modal ?? false);
      onCleanup(() => context!.unregister(id));
    },
  };
}
