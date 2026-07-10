import { createContext, useContext } from 'solid-js';
import type { JSX } from 'solid-js';

export interface LinkRenderProps {
  href: string;
  class?: string;
  target?: string;
  rel?: string;
  'aria-label'?: string;
  children?: JSX.Element;
}

export type LinkAdapter = (props: LinkRenderProps) => JSX.Element;

export const LinkAdapterContext = createContext<LinkAdapter | undefined>(undefined);

export function useLinkAdapter(): LinkAdapter | undefined {
  return useContext(LinkAdapterContext);
}
