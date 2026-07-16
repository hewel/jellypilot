import { css } from '@styled-system/css';

export const root = css({
  position: 'relative',
  overflow: 'hidden',
  borderRadius: '2xl',
  p: '4',
  boxShadow: 'inner',
  backdropFilter: '[blur(4px)]',
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
});

export const label = css({
  display: 'block',
  mb: '1',
  color: 'onSurfaceVariant',
  fontSize: '11',
  fontWeight: 'bold',
  lineHeight: '16',
  letterSpacing: '[0.08em]',
  opacity: '[0.9]',
  textTransform: 'uppercase',
});
