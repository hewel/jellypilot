import { css, cx } from '@styled-system/css';

export const textFieldRoot = css({});

export const textFieldLabel = css({
  display: 'block',
  mb: '1',
  ml: '1',
  color: 'onSurfaceVariant',
  fontSize: '12',
  fontWeight: 'bold',
  lineHeight: '16',
  letterSpacing: '[0.08em]',
  textTransform: 'uppercase',
  transitionDuration: '200',
  transitionProperty: '[color]',
  // Root owns focus-within; label is a descendant of the same owner.
  '.group:focus-within &': {
    color: 'primary',
  },
});

export const textFieldError = css({
  mt: '1_5',
  ml: '1',
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  animation: '[fadeIn 200ms {easings.emphasized} forwards]',
});

export const textFieldHint = css({
  mt: '1_5',
  ml: '1',
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
  opacity: '[0.7]',
});

export const fullWidth = css({
  width: 'full',
});

export { cx };
