import { projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';

export const toggleButton = style({
  minWidth: 0,
  paddingInline: projectTheme.space['3'],
});
