import { css } from '@styled-system/css';

export const mutedOutlinedButton = css({
  color: 'onSurfaceVariant',
  _hover: {
    borderColor: 'primary/50',
    color: 'onSurface',
  },
});

export const refreshButton = css({
  bg: 'surfaceContainerHigh/20',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  marginLeft: 'auto',
  _hover: {
    borderColor: 'secondary',
    color: 'secondary',
  },
});
