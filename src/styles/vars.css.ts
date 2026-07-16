import { token } from '@styled-system/tokens';
import { createGlobalTheme, createGlobalThemeContract } from '@vanilla-extract/css';

import {
  breakpoints as breakpointValues,
  durations,
  easings,
  fontSizes,
  fontWeights,
  fonts,
  legacyColorVarNames,
  legacyFontVarNames,
  legacyVarName,
  lineHeights,
  radii,
  shadows,
  spacing,
  zIndex,
} from './theme-tokens';

/**
 * Transitional vanilla-extract bridge.
 * Panda owns token values; this module only re-exports the legacy
 * `--jellypilot-*` contract as aliases to Panda-generated CSS variables.
 */

const tokenContract = <Token extends string>(group: string, values: Record<Token, string>) =>
  Object.fromEntries(Object.keys(values).map((key) => [key, legacyVarName(group, key)])) as Record<
    Token,
    string
  >;

// Panda's Token union is generated; map over known keys with a narrow cast.
const tokenVar = (path: string) => token.var(path as never);

export const breakpoints = breakpointValues;

export const vars = createGlobalThemeContract({
  color: legacyColorVarNames,
  font: legacyFontVarNames,
  space: tokenContract('space', spacing),
  fontSize: tokenContract('font-size', fontSizes),
  fontWeight: tokenContract('font-weight', fontWeights),
  lineHeight: tokenContract('line-height', lineHeights),
  borderRadius: tokenContract('radius', radii),
  shadow: tokenContract('shadow', shadows),
  zIndex: tokenContract('z-index', zIndex),
  duration: tokenContract('duration', durations),
  easing: tokenContract('easing', easings),
  breakpoint: tokenContract('breakpoint', breakpointValues),
});

const spaceAliases = Object.fromEntries(
  Object.keys(spacing).map((key) => [key, tokenVar(`spacing.${key}`)]),
) as Record<keyof typeof spacing, string>;

const fontSizeAliases = Object.fromEntries(
  Object.keys(fontSizes).map((key) => [key, tokenVar(`fontSizes.${key}`)]),
) as Record<keyof typeof fontSizes, string>;

const fontWeightAliases = Object.fromEntries(
  Object.keys(fontWeights).map((key) => [key, tokenVar(`fontWeights.${key}`)]),
) as Record<keyof typeof fontWeights, string>;

const lineHeightAliases = Object.fromEntries(
  Object.keys(lineHeights).map((key) => [key, tokenVar(`lineHeights.${key}`)]),
) as Record<keyof typeof lineHeights, string>;

const radiusAliases = Object.fromEntries(
  Object.keys(radii).map((key) => [key, tokenVar(`radii.${key}`)]),
) as Record<keyof typeof radii, string>;

const shadowAliases = Object.fromEntries(
  Object.keys(shadows).map((key) => [key, tokenVar(`shadows.${key}`)]),
) as Record<keyof typeof shadows, string>;

const zIndexAliases = Object.fromEntries(
  Object.keys(zIndex).map((key) => [key, tokenVar(`zIndex.${key}`)]),
) as Record<keyof typeof zIndex, string>;

const durationAliases = Object.fromEntries(
  Object.keys(durations).map((key) => [key, tokenVar(`durations.${key}`)]),
) as Record<keyof typeof durations, string>;

const easingAliases = Object.fromEntries(
  Object.keys(easings).map((key) => [key, tokenVar(`easings.${key}`)]),
) as Record<keyof typeof easings, string>;

const breakpointAliases = { ...breakpointValues };

const colorAliases = Object.fromEntries(
  Object.keys(legacyColorVarNames).map((key) => [key, tokenVar(`colors.${key}`)]),
) as Record<keyof typeof legacyColorVarNames, string>;

createGlobalTheme(':root', vars, {
  color: colorAliases,
  font: {
    display: tokenVar('fonts.display'),
    mono: tokenVar('fonts.mono'),
    sans: tokenVar('fonts.sans'),
  },
  space: spaceAliases,
  fontSize: fontSizeAliases,
  fontWeight: fontWeightAliases,
  lineHeight: lineHeightAliases,
  borderRadius: radiusAliases,
  shadow: shadowAliases,
  zIndex: zIndexAliases,
  duration: durationAliases,
  easing: easingAliases,
  // Breakpoints cannot be CSS variables in media queries; keep literal values.
  breakpoint: breakpointAliases,
});

// Keep fonts type-check reference so the bridge contract remains complete.
void fonts;
