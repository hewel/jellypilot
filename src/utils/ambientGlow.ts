import { createSignal } from 'solid-js';
import type { Accessor } from 'solid-js';

const GLOW_REGION_SELECTOR = '[data-sidebar],[data-toolbar]';

export interface AmbientGlow {
  active: Accessor<boolean>;
  onPointerOver: (event: PointerEvent) => void;
  onPointerOut: (event: PointerEvent) => void;
}

/**
 * Publishes whether any pointer is over a glow region (Sidebar or Library
 * Browser toolbar) so the authenticated shell can expose `data-glow`.
 * Delegated pointerover/pointerout on the `[data-shell]` div replace the
 * `:has(:hover)` selectors that forced root-scoped style recalculation on
 * every pointer move. DOM-only; the shell div's JSX props provide delegation.
 *
 * Occupancy is tracked per pointerId: pointerover is authoritative (entering
 * a region registers the pointer, pointerover on newly exposed non-region
 * content — e.g. after a hovered toolbar unmounts — clears it), and one
 * touch contact's pointerout never clears another contact still in a region.
 */
export function createAmbientGlow(): AmbientGlow {
  const pointers = new Set<number>();
  const [active, setActive] = createSignal(false);

  const onPointerOver = (event: PointerEvent): void => {
    if ((event.target as Element).closest(GLOW_REGION_SELECTOR) !== null) {
      pointers.add(event.pointerId);
    } else {
      pointers.delete(event.pointerId);
    }
    setActive(pointers.size > 0);
  };

  const onPointerOut = (event: PointerEvent): void => {
    // Null means the pointer left the window; a non-Element relatedTarget
    // (e.g. Window) counts as outside. Region-to-region moves keep the glow.
    const related = event.relatedTarget;
    if (!(related instanceof Element && related.closest(GLOW_REGION_SELECTOR) !== null)) {
      pointers.delete(event.pointerId);
      setActive(pointers.size > 0);
    }
  };

  return { active, onPointerOver, onPointerOut };
}
