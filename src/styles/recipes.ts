import { css, cva } from '@styled-system/css';

/** Modal/drawer scrim. `dark` = modal dialogs; `surface` = drawers & settings modal. */
export const scrim = cva({
  base: {
    bg: '[rgb(0 0 0 / 0.72)]',
    inset: '0',
    position: 'fixed',
    transitionDuration: 'overlay',
    transitionProperty: '[backdrop-filter, background-color, opacity]',
    '@supports (backdrop-filter: blur(1px))': {
      backdropFilter: '[blur(4px)]',
    },
    _motionReduce: {
      backdropFilter: '[none]',
      transitionDuration: '100',
    },
    '&[data-state="closed"]': { opacity: '[0]' },
    '&[data-state="open"]': { opacity: '[1]' },
  },
  variants: {
    tone: {
      dark: {},
      surface: { bg: 'materialDepthOverlay' },
    },
    z: {
      '50': { zIndex: '50' },
      '60': { zIndex: '60' },
      '100': { zIndex: '100' },
    },
  },
});

/** Centered modal positioner. `inset: 0` is baked in (all three consumers already pair positioner with the old positionerFill). */
export const modalPositioner = cva({
  base: {
    display: 'flex',
    inset: '0',
    justifyContent: 'center',
    p: '4',
    position: 'fixed',
  },
  variants: {
    align: {
      start: { alignItems: 'flex-start' },
      center: { alignItems: 'center' },
    },
    scroll: {
      true: { overflowY: 'auto' },
    },
    z: {
      '50': { zIndex: '50' },
      '60': { zIndex: '60' },
    },
  },
});

/** Visually hidden, screen-reader-only text. Canonical copy of OperationsConsole.styles srOnly. */
export const srOnly = css({
  border: 0,
  clip: '[rect(0 0 0 0)]',
  height: 'px',
  margin: '[-1px]',
  overflow: 'hidden',
  padding: '0',
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: 'px',
});

/** Shared 1.375rem checkbox box (Ark Checkbox consumers + LibrarySettingsCard decorative span). */
export const checkboxBox = css({
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: '[0]',
  color: 'onPrimary',
  fontSize: '11',
  lineHeight: 'none',
  borderRadius: 'lg',
  bg: 'surfaceContainerHigh',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outline',
  height: '[1.375rem]',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, box-shadow]',
  width: '[1.375rem]',
  _hover: {
    borderColor: 'primary/60',
  },
  '&[data-state="checked"], &[data-state="indeterminate"]': {
    bg: 'primary',
    borderColor: 'primary',
  },
  '&[data-focus-visible]': {
    outline: '[2px solid {colors.focusRing}]',
    outlineOffset: '[2px]',
  },
  _motionReduce: {
    transform: '[none]',
    transitionDuration: '100',
  },
});

export const checkboxIndicator = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontWeight: 'black',
});

/** Pill-shaped button clip. */
export const pillButton = css({
  borderRadius: 'full',
});

/**
 * The shared semantic material recipe (issue #169 contract): the single
 * appearance-aware surface vocabulary for reusable primitives. Consumes
 * semantic material tokens only — no raw palettes, no theme-specific
 * literals — so every Design Theme maps the same eight treatments through
 * its own token table.
 *
 * Components compose a treatment with their own recipe via
 * `css(material.raw({ treatment: '…' }), ownRecipe.raw(props))`, keeping
 * layout, typography, and state styles owner-local. No theme/material props
 * are exposed to callers.
 *
 * `acrylic` and `glass` are contrast-authoritative: the unconditional base is
 * an opaque semantic surface with a bounded semantic shadow; translucency and
 * blur are progressive enhancement inside `@supports (backdrop-filter)`, so
 * unsupported blur preserves the same hierarchy.
 */
export const material = cva({
  base: {},
  variants: {
    treatment: {
      /** Flush with the surrounding surface; separation comes from edges only. */
      flat: {
        bg: '[transparent]',
        borderColor: 'materialEdgeSubtle',
      },
      /** Panel above the canvas: opaque raised surface, subtle edge, raised depth. */
      raised: {
        bg: 'materialSurfaceRaised',
        borderColor: 'materialEdgeSubtle',
        boxShadow: '[0 1px 2px 0 {colors.materialDepthRaised}]',
      },
      /** Control well sunk below the surrounding surface; inner shadow at the top edge. */
      recessed: {
        bg: 'materialSurfaceRecessed',
        borderColor: 'materialEdgeSubtle',
        boxShadow: '[inset 0 2px 4px 0 {colors.materialDepthRecessed}]',
      },
      /**
       * Prominent overlay pane. The unconditional base is the opaque raised
       * surface — `materialSurfaceAcrylic` is translucent outside Control Room
       * Dark, so it only appears inside the backdrop-filter enhancement.
       */
      acrylic: {
        bg: 'materialSurfaceRaised',
        borderColor: 'materialEdgeSubtle',
        boxShadow: '[0 10px 18px -6px {colors.materialDepthAmbient}]',
        '@supports (backdrop-filter: blur(1px))': {
          backdropFilter: '[blur(16px)]',
          bg: 'materialSurfaceAcrylic/85',
        },
      },
      /**
       * Overlay pane (popovers, select content). The unconditional base is the
       * opaque raised surface — `materialSurfaceGlass` is translucent outside
       * Control Room Dark, so it only appears inside the backdrop-filter
       * enhancement.
       */
      glass: {
        bg: 'materialSurfaceRaised',
        borderColor: 'materialEdgeNormal',
        boxShadow: '[0 25px 50px -12px {colors.materialDepthOverlay}]',
        '@supports (backdrop-filter: blur(1px))': {
          backdropFilter: '[blur(12px)]',
          bg: 'materialSurfaceGlass/80',
        },
      },
      /** Tactile key at rest: opaque key surface, specular top edge, keycap depth. */
      keycap: {
        bg: 'materialSurfaceKey',
        borderColor: 'materialEdgeSubtle',
        boxShadow:
          '[inset 0 1px 0 0 {colors.materialEdgeSpecular}, 0 2px 4px 0 {colors.materialDepthKeycap}]',
      },
      /** Key mid-travel: pressed surface, inner depth, no raised shadow. */
      pressed: {
        bg: 'materialSurfacePressed',
        borderColor: 'materialEdgeNormal',
        boxShadow: '[inset 0 2px 4px 0 {colors.materialDepthPressed}]',
      },
      /** Steady status LED shape; color is carried by the status indicator tokens. */
      indicator: {
        bg: 'materialDepthIndicator',
        borderRadius: 'indicator',
      },
    },
  },
});

/**
 * Canonical visible focus treatment: 2px `focusRing` outline with offset.
 * Never primary/secondary — `focusRing` tracks each Design Theme and keeps
 * 3:1 against adjacent surfaces (token-contract test).
 */
export const focusRing = {
  outline: '[2px solid {colors.focusRing}]',
  outlineOffset: '[2px]',
} as const;

/**
 * Reduced-motion contract (issue #171): spatial transforms and mechanical
 * press/release motion are disabled; remaining non-spatial feedback is
 * capped at the 100ms duration token.
 */
export const reducedMotionFeedback = {
  transform: '[none]',
  transitionDuration: '100',
} as const;
