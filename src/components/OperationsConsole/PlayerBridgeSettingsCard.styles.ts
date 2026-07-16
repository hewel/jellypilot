import { css } from '@styled-system/css';

export const detectButton = css({
  minHeight: '14',
  sm: {
    minHeight: '[0]',
  },
});

export const advancedTrigger = css({
  fontWeight: 'bold',
  px: '0',
  _hover: {
    color: 'secondary',
  },
});

export const clearButton = css({
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  fontSize: '13',
  fontWeight: 'bold',
  minWidth: '[0]',
  py: '1',
  px: '3',
  _hover: {
    bg: 'secondary/5',
    borderColor: 'secondary',
  },
});

export const smallIconButton = css({
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
  borderRadius: 'lg',
  height: '8',
  width: '8',
  _hover: {
    borderColor: 'secondary',
    color: 'secondary',
  },
});

export const deleteButton = css({
  _hover: {
    borderColor: 'error',
    color: 'error',
  },
});
