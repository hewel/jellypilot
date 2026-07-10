export type ValueCategory =
  | 'token'
  | 'native'
  | 'numeric-spacing'
  | 'numeric-z'
  | 'numeric-opacity'
  | 'fraction'
  | 'arbitrary-length'
  | 'arbitrary-number'
  | 'arbitrary-color'
  | 'arbitrary-shadow'
  | 'arbitrary-grid'

export type FamilyDescriptor = {
  property: string
  cssProperty: string
  aliases?: string[]
  tokens?: Record<string, string>
  native?: readonly string[]
  categories: readonly ValueCategory[]
  /** Allow negative numeric values (margin/inset). */
  allowNegativeNumeric?: boolean
  order: number
  conflictsWith?: readonly string[]
}

const DISPLAY = [
  'block',
  'inline-block',
  'inline',
  'flex',
  'inline-flex',
  'grid',
  'inline-grid',
  'contents',
  'flow-root',
  'hidden',
  'none',
] as const

const POSITION = ['static', 'fixed', 'absolute', 'relative', 'sticky'] as const
const OVERFLOW = ['auto', 'hidden', 'clip', 'visible', 'scroll'] as const
const FLEX_DIRECTION = ['row', 'row-reverse', 'column', 'column-reverse'] as const
const FLEX_WRAP = ['wrap', 'wrap-reverse', 'nowrap'] as const
const ALIGN_ITEMS = [
  'start',
  'end',
  'center',
  'baseline',
  'stretch',
  'flex-start',
  'flex-end',
] as const
const JUSTIFY_CONTENT = [
  'start',
  'end',
  'center',
  'between',
  'around',
  'evenly',
  'stretch',
  'flex-start',
  'flex-end',
  'space-between',
  'space-around',
  'space-evenly',
] as const

const SPACING_CATEGORIES = [
  'token',
  'native',
  'numeric-spacing',
  'arbitrary-length',
] as const satisfies readonly ValueCategory[]

const SIZE_CATEGORIES = [
  'token',
  'native',
  'numeric-spacing',
  'fraction',
  'arbitrary-length',
] as const satisfies readonly ValueCategory[]

let order = 0
const nextOrder = (): number => {
  order += 1
  return order
}

export function createFamilyTable(tokens: {
  spacing: Record<string, string>
  width: Record<string, string>
  height: Record<string, string>
  maxWidth: Record<string, string>
  maxHeight: Record<string, string>
  minWidth: Record<string, string>
  minHeight: Record<string, string>
  zIndex: Record<string, string>
  colors: Record<string, string>
  fontSize: Record<string, string>
  fontWeight: Record<string, string>
  lineHeight: Record<string, string>
  borderRadius: Record<string, string>
  /** Project Theme shadows only — preset shared-variable shadows unsupported. */
  boxShadow: Record<string, string>
}): FamilyDescriptor[] {
  const spacingTokens = { ...tokens.spacing }
  const sizeWidth = { ...tokens.width }
  const sizeHeight = { ...tokens.height }

  return [
    {
      property: 'display',
      cssProperty: 'display',
      native: DISPLAY,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'position',
      cssProperty: 'position',
      native: POSITION,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'overflow',
      cssProperty: 'overflow',
      native: OVERFLOW,
      categories: ['native'],
      order: nextOrder(),
      conflictsWith: ['overflowX', 'overflowY'],
    },
    {
      property: 'overflowX',
      cssProperty: 'overflow-x',
      native: OVERFLOW,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'overflowY',
      cssProperty: 'overflow-y',
      native: OVERFLOW,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'flexDirection',
      cssProperty: 'flex-direction',
      aliases: ['direction'],
      native: FLEX_DIRECTION,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'flexWrap',
      cssProperty: 'flex-wrap',
      aliases: ['wrap'],
      native: FLEX_WRAP,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'flexGrow',
      cssProperty: 'flex-grow',
      aliases: ['grow'],
      native: ['0', '1'],
      categories: ['native', 'numeric-z', 'arbitrary-number'],
      order: nextOrder(),
    },
    {
      property: 'flexShrink',
      cssProperty: 'flex-shrink',
      aliases: ['shrink'],
      native: ['0', '1'],
      categories: ['native', 'numeric-z', 'arbitrary-number'],
      order: nextOrder(),
    },
    {
      property: 'alignItems',
      cssProperty: 'align-items',
      aliases: ['items'],
      native: ALIGN_ITEMS,
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'justifyContent',
      cssProperty: 'justify-content',
      aliases: ['justify'],
      native: JUSTIFY_CONTENT,
      categories: ['native'],
      order: nextOrder(),
    },
    spacingFamily('gap', 'gap', spacingTokens),
    spacingFamily('columnGap', 'column-gap', spacingTokens, ['gapX']),
    spacingFamily('rowGap', 'row-gap', spacingTokens, ['gapY']),
    spacingFamily('padding', 'padding', spacingTokens, ['p'], {
      conflictsWith: [
        'paddingX',
        'paddingY',
        'paddingTop',
        'paddingRight',
        'paddingBottom',
        'paddingLeft',
      ],
    }),
    spacingFamily('paddingX', 'padding-inline', spacingTokens, ['px'], {
      conflictsWith: ['paddingLeft', 'paddingRight'],
    }),
    spacingFamily('paddingY', 'padding-block', spacingTokens, ['py'], {
      conflictsWith: ['paddingTop', 'paddingBottom'],
    }),
    spacingFamily('paddingTop', 'padding-top', spacingTokens, ['pt']),
    spacingFamily('paddingRight', 'padding-right', spacingTokens, ['pr']),
    spacingFamily('paddingBottom', 'padding-bottom', spacingTokens, ['pb']),
    spacingFamily('paddingLeft', 'padding-left', spacingTokens, ['pl']),
    spacingFamily('margin', 'margin', spacingTokens, ['m'], {
      allowNegativeNumeric: true,
      conflictsWith: [
        'marginX',
        'marginY',
        'marginTop',
        'marginRight',
        'marginBottom',
        'marginLeft',
      ],
    }),
    spacingFamily('marginX', 'margin-inline', spacingTokens, ['mx'], {
      allowNegativeNumeric: true,
      conflictsWith: ['marginLeft', 'marginRight'],
    }),
    spacingFamily('marginY', 'margin-block', spacingTokens, ['my'], {
      allowNegativeNumeric: true,
      conflictsWith: ['marginTop', 'marginBottom'],
    }),
    spacingFamily('marginTop', 'margin-top', spacingTokens, ['mt'], {
      allowNegativeNumeric: true,
    }),
    spacingFamily('marginRight', 'margin-right', spacingTokens, ['mr'], {
      allowNegativeNumeric: true,
    }),
    spacingFamily('marginBottom', 'margin-bottom', spacingTokens, ['mb'], {
      allowNegativeNumeric: true,
    }),
    spacingFamily('marginLeft', 'margin-left', spacingTokens, ['ml'], {
      allowNegativeNumeric: true,
    }),
    spacingFamily('inset', 'inset', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      conflictsWith: ['top', 'right', 'bottom', 'left', 'insetX', 'insetY'],
    }),
    spacingFamily('insetX', 'inset-inline', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      conflictsWith: ['left', 'right'],
    }),
    spacingFamily('insetY', 'inset-block', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      conflictsWith: ['top', 'bottom'],
    }),
    spacingFamily('top', 'top', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      categories: SIZE_CATEGORIES,
    }),
    spacingFamily('right', 'right', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      categories: SIZE_CATEGORIES,
    }),
    spacingFamily('bottom', 'bottom', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      categories: SIZE_CATEGORIES,
    }),
    spacingFamily('left', 'left', spacingTokens, undefined, {
      allowNegativeNumeric: true,
      categories: SIZE_CATEGORIES,
    }),
    {
      property: 'width',
      cssProperty: 'width',
      tokens: sizeWidth,
      native: ['auto', 'full', 'screen', 'min', 'max', 'fit'],
      categories: SIZE_CATEGORIES,
      order: nextOrder(),
    },
    {
      property: 'height',
      cssProperty: 'height',
      tokens: sizeHeight,
      native: ['auto', 'full', 'screen', 'min', 'max', 'fit'],
      categories: SIZE_CATEGORIES,
      order: nextOrder(),
    },
    {
      property: 'minWidth',
      cssProperty: 'min-width',
      tokens: { ...tokens.minWidth },
      native: ['0', 'full', 'min', 'max', 'fit'],
      categories: SIZE_CATEGORIES,
      order: nextOrder(),
    },
    {
      property: 'maxWidth',
      cssProperty: 'max-width',
      tokens: { ...tokens.maxWidth },
      native: ['none', 'full', 'min', 'max', 'fit', 'prose'],
      categories: SIZE_CATEGORIES,
      order: nextOrder(),
    },
    {
      property: 'minHeight',
      cssProperty: 'min-height',
      tokens: { ...tokens.minHeight },
      native: ['0', 'full', 'screen', 'min', 'max', 'fit'],
      categories: SIZE_CATEGORIES,
      order: nextOrder(),
    },
    {
      property: 'maxHeight',
      cssProperty: 'max-height',
      tokens: { ...tokens.maxHeight },
      native: ['none', 'full', 'screen', 'min', 'max', 'fit'],
      categories: SIZE_CATEGORIES,
      order: nextOrder(),
    },
    {
      property: 'zIndex',
      cssProperty: 'z-index',
      aliases: ['z'],
      tokens: { ...tokens.zIndex },
      native: ['auto'],
      categories: ['token', 'native', 'numeric-z', 'arbitrary-number'],
      allowNegativeNumeric: true,
      order: nextOrder(),
    },
    {
      property: 'color',
      cssProperty: 'color',
      aliases: ['text'],
      tokens: { ...tokens.colors },
      categories: ['token', 'arbitrary-color'],
      order: nextOrder(),
    },
    {
      property: 'backgroundColor',
      cssProperty: 'background-color',
      aliases: ['bg'],
      tokens: { ...tokens.colors },
      categories: ['token', 'arbitrary-color'],
      order: nextOrder(),
    },
    {
      property: 'borderColor',
      cssProperty: 'border-color',
      tokens: { ...tokens.colors },
      categories: ['token', 'arbitrary-color'],
      order: nextOrder(),
    },
    {
      property: 'fontSize',
      cssProperty: 'font-size',
      tokens: { ...tokens.fontSize },
      categories: ['token', 'arbitrary-length'],
      order: nextOrder(),
    },
    {
      property: 'fontWeight',
      cssProperty: 'font-weight',
      tokens: { ...tokens.fontWeight },
      categories: ['token', 'numeric-z', 'arbitrary-number'],
      order: nextOrder(),
    },
    {
      property: 'lineHeight',
      cssProperty: 'line-height',
      aliases: ['leading'],
      tokens: { ...tokens.lineHeight },
      categories: ['token', 'numeric-spacing', 'arbitrary-number', 'arbitrary-length'],
      order: nextOrder(),
    },
    {
      property: 'textAlign',
      cssProperty: 'text-align',
      native: ['left', 'center', 'right', 'justify', 'start', 'end'],
      categories: ['native'],
      order: nextOrder(),
    },
    {
      property: 'borderRadius',
      cssProperty: 'border-radius',
      aliases: ['rounded'],
      tokens: { ...tokens.borderRadius },
      categories: ['token', 'arbitrary-length'],
      order: nextOrder(),
    },
    {
      property: 'boxShadow',
      cssProperty: 'box-shadow',
      aliases: ['shadow'],
      tokens: { ...tokens.boxShadow },
      categories: ['token', 'arbitrary-shadow'],
      order: nextOrder(),
    },
    {
      property: 'opacity',
      cssProperty: 'opacity',
      categories: ['numeric-opacity', 'arbitrary-number'],
      order: nextOrder(),
    },
    {
      property: 'gridTemplateColumns',
      cssProperty: 'grid-template-columns',
      aliases: ['gridCols'],
      categories: ['arbitrary-grid'],
      order: nextOrder(),
    },
    {
      property: 'gridTemplateRows',
      cssProperty: 'grid-template-rows',
      aliases: ['gridRows'],
      categories: ['arbitrary-grid'],
      order: nextOrder(),
    },
  ]
}

function spacingFamily(
  property: string,
  cssProperty: string,
  tokens: Record<string, string>,
  aliases?: string[],
  extra?: Partial<FamilyDescriptor>,
): FamilyDescriptor {
  return {
    property,
    cssProperty,
    aliases,
    tokens: { ...tokens },
    native: ['auto'],
    categories: extra?.categories ?? SPACING_CATEGORIES,
    allowNegativeNumeric: extra?.allowNegativeNumeric,
    order: nextOrder(),
    conflictsWith: extra?.conflictsWith,
  }
}
