import { createSignal } from 'solid-js';

const [imageProxyBase, setImageProxyBaseSignal] = createSignal<string | null>(null);

export function setImageProxyBase(base: string | null): void {
  setImageProxyBaseSignal(base?.replace(/\/+$/, '') ?? null);
}

export function resetImageProxyBase(): void {
  setImageProxyBaseSignal(null);
}

export function imageSource(imageId: string): string {
  const base = imageProxyBase();
  return base ? `${base}/image/${encodeURIComponent(imageId)}` : '';
}
