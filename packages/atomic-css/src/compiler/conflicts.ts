import type { AtomicDeclaration } from './extract.js'

type Scope = 'side' | 'axis' | 'all'

const PADDING_SCOPE: Record<string, Scope> = {
  padding: 'all',
  'padding-inline': 'axis',
  'padding-block': 'axis',
  'padding-left': 'side',
  'padding-right': 'side',
  'padding-top': 'side',
  'padding-bottom': 'side',
}

const MARGIN_SCOPE: Record<string, Scope> = {
  margin: 'all',
  'margin-inline': 'axis',
  'margin-block': 'axis',
  'margin-left': 'side',
  'margin-right': 'side',
  'margin-top': 'side',
  'margin-bottom': 'side',
}

const INSET_SCOPE: Record<string, Scope> = {
  inset: 'all',
  'inset-inline': 'axis',
  'inset-block': 'axis',
  left: 'side',
  right: 'side',
  top: 'side',
  bottom: 'side',
}

const SCOPE_RANK: Record<Scope, number> = {
  side: 3,
  axis: 2,
  all: 1,
}

/** Expand-then-resolve with side > axis > all; equal-scope conflicts fail. */
export function resolveConflicts(
  declarations: AtomicDeclaration[],
): AtomicDeclaration[] {
  const byProperty = new Map<string, string>()
  const order: string[] = []

  for (const declaration of declarations) {
    const existing = byProperty.get(declaration.property)
    if (existing === undefined) {
      byProperty.set(declaration.property, declaration.value)
      order.push(declaration.property)
      continue
    }
    if (existing === declaration.value) {
      continue
    }
    throw new Error(
      `equal-scope conflict for ${declaration.property}: ${existing} vs ${declaration.value}`,
    )
  }

  applyShorthandPrecedence(byProperty, PADDING_SCOPE)
  applyShorthandPrecedence(byProperty, MARGIN_SCOPE)
  applyShorthandPrecedence(byProperty, INSET_SCOPE)

  return [...byProperty.entries()].map(([property, value]) => ({
    property,
    value,
  }))
}

function applyShorthandPrecedence(
  byProperty: Map<string, string>,
  scopes: Record<string, Scope>,
): void {
  const present = Object.keys(scopes).filter((property) =>
    byProperty.has(property),
  )
  if (present.length <= 1) return

  let winnerRank = 0
  for (const property of present) {
    const rank = SCOPE_RANK[scopes[property]!]
    if (rank > winnerRank) winnerRank = rank
  }

  for (const property of present) {
    const rank = SCOPE_RANK[scopes[property]!]
    if (rank < winnerRank) {
      byProperty.delete(property)
    }
  }

  // Equal-scope different properties can coexist (padding-left + padding-right).
  // Equal-scope same property already failed above.
}
