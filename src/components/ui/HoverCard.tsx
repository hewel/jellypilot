import { computePosition, flip, offset, shift } from '@floating-ui/dom';
import type { Placement } from '@floating-ui/dom';
import { createEffect, createSignal, onCleanup, Show } from 'solid-js';
import type { JSX } from 'solid-js';
import { Portal } from 'solid-js/web';

import * as styles from './HoverCard.styles';

export interface HoverCardProps {
  /** Delay before hover opens the card. Defaults to 250ms. */
  openDelay?: number;
  /** Grace period before the card closes after pointer/focus leaves. Defaults to 300ms. */
  closeDelay?: number;
  /** Floating-ui placement relative to the trigger. Defaults to 'top'. */
  placement?: Placement;
  /** Gap between trigger and card in pixels. Defaults to 10. */
  gutter?: number;
  /** Called on every open/close transition; use it to gate lazy data fetching. */
  onOpenChange?: (open: boolean) => void;
  /** Card body. Rendered inside a Portal only while the card is open. */
  content: () => JSX.Element;
  /** Trigger content. Wrapped in a div that owns hover/focus intent. */
  children: JSX.Element;
}

/**
 * Hand-rolled hover/focus card positioned with @floating-ui/dom. Replaces the
 * Ark UI hover-card, whose zag-js state machine anchors incorrectly when its
 * trigger lives inside a virtualized grid (rows are recycled and re-positioned).
 *
 * Behavior contract:
 * - opens on hover after `openDelay`, or immediately on keyboard focus
 * - closes after `closeDelay` once neither pointer nor focus is inside
 * - moving the pointer from trigger into the card keeps it open
 * - Escape closes immediately; any scroll in the document closes immediately
 * - content only mounts while open, so `onOpenChange` can gate fetching
 */
export function HoverCard(props: HoverCardProps) {
  const [open, setOpen] = createSignal(false);

  let triggerEl: HTMLDivElement | undefined;
  let pointerInside = false;
  let focusInside = false;
  let openTimer: number | undefined;
  let closeTimer: number | undefined;

  const openDelay = () => props.openDelay ?? 250;
  const closeDelay = () => props.closeDelay ?? 300;

  const clearTimers = () => {
    clearTimeout(openTimer);
    clearTimeout(closeTimer);
    openTimer = undefined;
    closeTimer = undefined;
  };

  const setOpenState = (next: boolean) => {
    if (open() === next) return;
    setOpen(next);
    props.onOpenChange?.(next);
  };

  const openNow = () => {
    clearTimers();
    setOpenState(true);
  };

  const scheduleOpen = () => {
    clearTimeout(openTimer);
    openTimer = window.setTimeout(() => {
      openTimer = undefined;
      setOpenState(true);
    }, openDelay());
  };

  const scheduleClose = () => {
    clearTimeout(closeTimer);
    closeTimer = window.setTimeout(() => {
      closeTimer = undefined;
      if (!pointerInside && !focusInside) {
        setOpenState(false);
      }
    }, closeDelay());
  };

  const onTriggerPointerEnter = () => {
    pointerInside = true;
    clearTimeout(closeTimer);
    if (!open() && openTimer === undefined) {
      scheduleOpen();
    }
  };

  const onTriggerPointerLeave = () => {
    pointerInside = false;
    clearTimeout(openTimer);
    openTimer = undefined;
    if (open()) {
      scheduleClose();
    }
  };

  const onContentPointerEnter = () => {
    pointerInside = true;
    clearTimeout(closeTimer);
  };

  const onContentPointerLeave = () => {
    pointerInside = false;
    scheduleClose();
  };

  const onTriggerFocusIn = () => {
    focusInside = true;
    openNow();
  };

  const onTriggerFocusOut = () => {
    focusInside = false;
    scheduleClose();
  };
  /**
    Global dismiss listeners exist only while the card is open. Scroll uses
    capture phase because scroll events do not bubble; this catches the
    app scroll area viewport as well as any nested scroller. 
  **/
  createEffect(() => {
    if (!open()) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        clearTimers();
        pointerInside = false;
        focusInside = false;
        setOpenState(false);
      }
    };
    const onScroll = () => {
      clearTimers();
      pointerInside = false;
      focusInside = false;
      setOpenState(false);
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('scroll', onScroll, { capture: true, passive: true });
    onCleanup(() => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('scroll', onScroll, { capture: true });
    });
  });

  onCleanup(clearTimers);

  const positionContent = (contentEl: HTMLDivElement) => {
    if (!triggerEl) return;
    const trigger = triggerEl;
    void computePosition(trigger, contentEl, {
      placement: props.placement ?? 'top',
      middleware: [offset(props.gutter ?? 10), flip(), shift({ padding: 8 })],
    }).then(({ x, y }) => {
      Object.assign(contentEl.style, { left: `${x}px`, top: `${y}px` });
    });
  };

  return (
    <>
      <div
        ref={triggerEl}
        onPointerEnter={onTriggerPointerEnter}
        onPointerLeave={onTriggerPointerLeave}
        onFocusIn={onTriggerFocusIn}
        onFocusOut={onTriggerFocusOut}
      >
        {props.children}
      </div>
      <Show when={open()}>
        <Portal>
          <div
            ref={positionContent}
            class={styles.card}
            style={{ left: '0', position: 'absolute', top: '0' }}
            onPointerEnter={onContentPointerEnter}
            onPointerLeave={onContentPointerLeave}
          >
            {props.content()}
          </div>
        </Portal>
      </Show>
    </>
  );
}

export default HoverCard;
