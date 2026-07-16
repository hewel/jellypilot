import { css } from '@styled-system/css';

export const root = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  pb: '4',
});

export const title = css({
  color: 'onSurface',
  fontSize: '32',
  fontWeight: 'bold',
  lineHeight: '40',
  fontFamily: 'display',
});

export const description = css({
  mt: '1',
  color: 'onSurfaceVariant',
  fontSize: '16',
  lineHeight: '24',
});
