import { css, cva } from '@styled-system/css';

/** Modal/drawer scrim. `dark` = modal dialogs; `surface` = drawers & settings modal. */
export const scrim = cva({
  base: {
    backdropFilter: '[blur(4px)]',
    inset: '0',
    position: 'fixed',
    transitionDuration: '300',
    transitionProperty: '[backdrop-filter, background-color, opacity]',
    '&[data-state="closed"]': { opacity: '[0]' },
    '&[data-state="open"]': { opacity: '[1]' },
  },
  variants: {
    tone: {
      dark: { bg: '[rgb(0 0 0 / 0.7)]' },
      surface: { bg: 'surfaceContainerLowest/70' },
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
    boxShadow: '[0 0 0 2px {colors.primary/50}]',
    outline: 'none',
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
