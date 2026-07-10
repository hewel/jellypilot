export type ConditionKind =
  | 'breakpoint'
  | 'hover'
  | 'focus'
  | 'focus-visible'
  | 'active'
  | 'dark'
  | 'disabled'
  | 'aria'
  | 'data'

export type Condition = {
  kind: ConditionKind
  key: string
  rank: number
}

const BREAKPOINT_RANK: Record<string, number> = {
  sm: 10,
  md: 20,
  lg: 30,
  xl: 40,
  '2xl': 50,
}

const INTERACTION_RANK: Record<string, number> = {
  hover: 60,
  focus: 70,
  'focus-visible': 80,
  active: 90,
}

/** Normalize condition chain by category/rank, not author nesting order. */
export function canonicalizeConditions(keys: string[]): Condition[] {
  const conditions: Condition[] = []
  for (const key of keys) {
    if (key in BREAKPOINT_RANK) {
      conditions.push({
        kind: 'breakpoint',
        key,
        rank: BREAKPOINT_RANK[key]!,
      })
      continue
    }
    if (key in INTERACTION_RANK) {
      conditions.push({
        kind: key as Condition['kind'],
        key,
        rank: INTERACTION_RANK[key]!,
      })
      continue
    }
    if (key === 'dark') {
      conditions.push({ kind: 'dark', key, rank: 100 })
      continue
    }
    if (key === 'disabled') {
      conditions.push({ kind: 'disabled', key, rank: 110 })
      continue
    }
    if (key.startsWith('aria-')) {
      conditions.push({ kind: 'aria', key, rank: 120 })
      continue
    }
    if (key.startsWith('data-')) {
      conditions.push({ kind: 'data', key, rank: 130 })
      continue
    }
    throw new Error(`unsupported-condition: ${key}`)
  }

  // Collapse duplicate predicates.
  const seen: Record<string, true> = {}
  const unique: Condition[] = []
  for (const condition of conditions) {
    if (seen[condition.key]) continue
    seen[condition.key] = true
    unique.push(condition)
  }

  // Incompatible: multiple breakpoints.
  const breakpoints = unique.filter((c) => c.kind === 'breakpoint')
  if (breakpoints.length > 1) {
    throw new Error(
      `incompatible-breakpoints: ${breakpoints.map((c) => c.key).join(', ')}`,
    )
  }

  return unique.sort((a, b) => a.rank - b.rank || a.key.localeCompare(b.key))
}

export function conditionSelector(
  conditions: Condition[],
  options: { dark?: string; disabled?: string; breakpoints: Record<string, string> },
): { media?: string; selectorSuffix: string } {
  let media: string | undefined
  const suffixes: string[] = []
  for (const condition of conditions) {
    if (condition.kind === 'breakpoint') {
      const width = options.breakpoints[condition.key]
      if (!width) throw new Error(`unknown breakpoint: ${condition.key}`)
      if (width.startsWith('var(')) {
        throw new Error('breakpoint values must be plain build-time lengths')
      }
      media = `(min-width: ${width})`
      continue
    }
    if (condition.kind === 'hover') suffixes.push('&:hover')
    else if (condition.kind === 'focus') suffixes.push('&:focus')
    else if (condition.kind === 'focus-visible') suffixes.push('&:focus-visible')
    else if (condition.kind === 'active') suffixes.push('&:active')
    else if (condition.kind === 'dark') {
      suffixes.push(options.dark ?? "[data-theme='dark'] &")
    } else if (condition.kind === 'disabled') {
      suffixes.push(options.disabled ?? '&:disabled')
    } else if (condition.kind === 'aria') {
      suffixes.push(`&[${condition.key}='true']`)
    } else if (condition.kind === 'data') {
      suffixes.push(`&[${condition.key}]`)
    }
  }
  return { media, selectorSuffix: suffixes.join('') }
}
