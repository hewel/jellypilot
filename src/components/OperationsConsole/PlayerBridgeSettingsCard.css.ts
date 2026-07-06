import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const saveBadge = style([
  sprinkles({
    borderRadius: 'sm',
    px: '2_5',
    py: '0_5',
    fontSize: '11',
    fontWeight: 'bold',
  }),
  {
    border: '1px solid transparent',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const saveOk = style({
  background: mix(vars.color.secondaryContainer, 0.2),
  borderColor: mix(vars.color.secondary, 0.2),
  color: vars.color.secondary,
});

export const saveError = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.errorContainer, 0.2),
  borderColor: mix(vars.color.error, 0.2),
  color: vars.color.error,
});

export const field = style({
  display: 'block',
});

export const error = sprinkles({
  mt: '1_5',
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
});

export const helper = style([
  sprinkles({
    mt: '1_5',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);

export const detectRow = style({
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['2_5'],
  '@media': {
    'screen and (min-width: 640px)': {
      flexDirection: 'row',
    },
  },
});

export const flexInput = style({
  flex: 1,
  minWidth: 0,
});

export const detectButton = style({
  minHeight: vars.space['14'],
  '@media': {
    'screen and (min-width: 640px)': {
      minHeight: 0,
    },
  },
});

export const advancedTrigger = style({
  fontWeight: vars.fontWeight.bold,
  paddingInline: 0,
  selectors: {
    '&:hover': {
      color: vars.color.secondary,
    },
  },
});

export const chevron = style({
  color: vars.color.onSurfaceVariant,
  height: '1.125rem',
  transitionDuration: vars.duration['300'],
  transitionProperty: 'color, transform',
  width: '1.125rem',
});

export const chevronOpen = style({
  color: vars.color.secondary,
  transform: 'rotate(180deg)',
});

export const advancedPanel = style([
  sprinkles({
    mt: '3',
    borderRadius: '2xl',
    p: '4',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerLowest, 0.3),
    border: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const subheading = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
});

export const subheadingAccent = style({
  background: vars.color.secondary,
  borderRadius: vars.borderRadius.md,
  height: vars.space['3'],
  width: vars.space['1'],
});

export const textarea = style({
  color: mix(vars.color.onSurfaceVariant, 0.8),
  fontFamily: vars.font.mono,
  fontSize: vars.fontSize['12'],
  height: 'auto',
  lineHeight: vars.lineHeight['16'],
  paddingBlock: vars.space['3_5'],
  width: '100%',
});

export const languagePanel = style([
  sprinkles({
    position: 'relative',
    borderRadius: '2xl',
    p: '5',
    boxShadow: 'inner',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerHigh, 0.3),
    border: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
  },
]);

export const panelHeader = style({
  alignItems: 'flex-start',
  display: 'flex',
  flexWrap: 'wrap',
  gap: vars.space['4'],
  justifyContent: 'space-between',
});

export const languageTitle = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const languageIcon = style({
  color: vars.color.secondary,
  height: '1.125rem',
  width: '1.125rem',
});

export const clearButton = style({
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.xl,
  fontSize: vars.fontSize['13'],
  fontWeight: vars.fontWeight.bold,
  minWidth: 0,
  padding: `${vars.space['1']} ${vars.space['3']}`,
  selectors: {
    '&:hover': {
      background: mix(vars.color.secondary, 0.05),
      borderColor: vars.color.secondary,
    },
  },
});

export const hidden = style({
  display: 'none',
});

export const languageGrid = style({
  display: 'grid',
  gap: vars.space['4'],
  gridTemplateColumns: '1fr',
  marginTop: vars.space['5'],
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
    },
  },
});

export const customField = sprinkles({
  display: 'flex',
  minWidth: '0',
  flexDirection: 'column',
});

export const addRow = sprinkles({
  display: 'flex',
  gap: '2',
});

export const addButton = style([
  sprinkles({
    display: 'inlineFlex',
    height: '14',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: '2xl',
    px: '4',
    color: 'onSecondaryContainer',
    fontSize: '14',
    lineHeight: '20',
    fontWeight: 'bold',
  }),
  {
    background: mix(vars.color.secondaryContainer, 0.4),
    border: `1px solid ${mix(vars.color.secondary, 0.2)}`,
    cursor: 'pointer',
    minWidth: '5.5rem',
    transitionDuration: vars.duration['200'],
    transitionProperty: 'background-color, border-color, transform',
    selectors: {
      '&:hover': {
        background: mix(vars.color.secondaryContainer, 0.6),
        borderColor: mix(vars.color.secondary, 0.4),
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
      '&:disabled': {
        opacity: 0.5,
        pointerEvents: 'none',
      },
    },
  },
]);

export const plusIcon = style({
  height: vars.space['4'],
  marginRight: vars.space['1'],
  width: vars.space['4'],
});

export const empty = style([
  sprinkles({
    mt: '5',
    borderRadius: '2xl',
    px: '4',
    py: '4',
    textAlign: 'center',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerLowest, 0.2),
    border: `1px dashed ${vars.color.outlineVariant}`,
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);

export const list = sprinkles({
  position: 'relative',
  mt: '5',
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  zIndex: '10',
});

export const item = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '3',
    borderRadius: 'xl',
    px: '4',
    py: '2_5',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerLowest, 0.6),
    border: `1px solid ${vars.color.outlineVariant}`,
    transitionProperty: 'background-color',
    selectors: {
      '&:hover': {
        background: mix(vars.color.surfaceContainerLowest, 0.8),
      },
    },
  },
]);

export const itemPreview = sprinkles({
  display: 'flex',
  minWidth: '0',
  flexGrow: '1',
  alignItems: 'center',
  gap: '3',
});

export const indexBadge = style([
  sprinkles({
    display: 'flex',
    height: '6',
    width: '6',
    flexShrink: '0',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 'lg',
    color: 'secondary',
    fontSize: '11',
    fontWeight: 'bold',
    boxShadow: 'inner',
  }),
  {
    background: mix(vars.color.surfaceContainerHigh, 0.6),
    border: `1px solid ${vars.color.outlineVariant}`,
    fontFamily: vars.font.mono,
    fontVariantNumeric: 'tabular-nums',
  },
]);

export const code = style({
  color: vars.color.onSurface,
  flexShrink: 0,
  fontFamily: vars.font.mono,
  fontSize: vars.fontSize['14'],
  fontWeight: vars.fontWeight.bold,
});

export const itemLabel = style([
  sprinkles({
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const itemActions = sprinkles({
  display: 'flex',
  flexShrink: '0',
  alignItems: 'center',
  gap: '1_5',
});

export const smallIconButton = style({
  background: mix(vars.color.surfaceContainerHigh, 0.3),
  border: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
  borderRadius: vars.borderRadius.lg,
  height: vars.space['8'],
  width: vars.space['8'],
  selectors: {
    '&:hover': {
      borderColor: vars.color.secondary,
      color: vars.color.secondary,
    },
  },
});

export const deleteButton = style({
  selectors: {
    '&:hover': {
      borderColor: vars.color.error,
      color: vars.color.error,
    },
  },
});
