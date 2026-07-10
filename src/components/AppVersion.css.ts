import { atomic } from '@jellypilot/atomic-css';
import { projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';

export const version = style([
  atomic({
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-11)',
    fontWeight: '700',
    lineHeight: 'var(--jellypilot-line-height-16)',
    marginTop: '0.25rem',
  }),
  {
    fontFamily: projectTheme.font.mono,
    letterSpacing: '0.08em',
    opacity: 0.5,
    textTransform: 'uppercase',
  },
]);
