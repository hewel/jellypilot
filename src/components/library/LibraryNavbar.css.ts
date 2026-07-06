import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const root = style([
  sprinkles({
    position: 'sticky',
    borderRadius: '2xl',
    boxShadow: 'xl',
    zIndex: '100',
  }),
  {
    backdropFilter: 'blur(12px)',
    background: `color-mix(in srgb, ${vars.color.surfaceContainerLow} 75%, transparent)`,
    border: `1px solid ${vars.color.outlineVariant}`,
    top: vars.space['2'],
  },
]);

export const inner = sprinkles({
  display: 'flex',
  flexDirection: 'row',
  flexWrap: 'wrap',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: { base: '2', sm: '4' },
});

export const segments = sprinkles({
  position: 'relative',
  display: 'flex',
  minWidth: '0',
  flexWrap: 'wrap',
  gap: '1',
  borderRadius: 'xl',
  p: '1',
});

export const indicator = style({
  background: vars.color.secondaryContainer,
  borderRadius: vars.borderRadius.lg,
  bottom: 'var(--bottom)',
  boxShadow: vars.shadow.sm,
  height: 'var(--height)',
  left: 'var(--left)',
  position: 'absolute',
  right: 'var(--right)',
  top: 'var(--top)',
  width: 'var(--width)',
});

export const item = style([
  sprinkles({
    position: 'relative',
    display: 'inlineFlex',
    height: '10',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 'lg',
    px: '4',
    color: 'onSurfaceVariant',
    fontSize: '14',
    lineHeight: '20',
    fontWeight: 'bold',
    zIndex: '10',
  }),
  {
    cursor: 'pointer',
    transitionProperty: 'color',
    selectors: {
      '&:hover': {
        color: vars.color.onSurface,
      },
      '&[data-state="checked"]': {
        color: vars.color.onSecondaryContainer,
      },
      '&[data-disabled]': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const homeIcon = style({
  height: '1.125rem',
  width: '1.125rem',
});

export const srOnly = style({
  border: 0,
  clip: 'rect(0 0 0 0)',
  height: '1px',
  margin: '-1px',
  overflow: 'hidden',
  padding: 0,
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: '1px',
});

export const portalTarget = sprinkles({
  display: 'flex',
  minWidth: '0',
  flexGrow: '1',
  justifyContent: 'flex-end',
});
