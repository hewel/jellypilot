import { resolveConflicts } from './conflicts.js'
import { buildAtomicSchema, resolveDeclaration } from '../schema/schema.js'
import type { AtomicSchema } from '../schema/schema.js'

export type AtomicDeclaration = {
  property: string
  value: string
}

export type ExtractedAtomicCall = {
  start: number
  end: number
  declarations: AtomicDeclaration[]
}

const ATOMIC_CALL = /atomic\s*\(\s*(\{[\s\S]*?\})\s*\)/g

let activeSchema: AtomicSchema = buildAtomicSchema({ preset: 'preset-mini' })

export function setActiveSchema(schema: AtomicSchema): void {
  activeSchema = schema
}

export function getActiveSchema(): AtomicSchema {
  return activeSchema
}

/** Minimal static parser for object-literal atomic() calls. */
export function extractAtomicCalls(source: string): ExtractedAtomicCall[] {
  const calls: ExtractedAtomicCall[] = []
  for (const match of source.matchAll(ATOMIC_CALL)) {
    const full = match[0]
    const objectLiteral = match[1]
    if (objectLiteral === undefined || match.index === undefined) continue
    const declarations = resolveConflicts(parseStaticObject(objectLiteral))
    if (declarations.length === 0) {
      throw new Error('atomic() calls must declare at least one property')
    }
    calls.push({
      start: match.index,
      end: match.index + full.length,
      declarations,
    })
  }
  return calls
}

function parseStaticObject(objectLiteral: string): AtomicDeclaration[] {
  // ponytail: only bare identifiers + string/number literals for this slice
  const body = objectLiteral.trim().replace(/^\{|\}$/g, '')
  if (body.trim().length === 0) {
    throw new Error('atomic() calls must not be empty')
  }
  const declarations: AtomicDeclaration[] = []
  for (const part of body.split(',')) {
    const trimmed = part.trim()
    if (trimmed.length === 0) continue
    const colon = trimmed.indexOf(':')
    if (colon === -1) {
      throw new Error(`unsupported atomic() entry: ${trimmed}`)
    }
    const rawKey = trimmed.slice(0, colon).trim()
    const rawValue = trimmed.slice(colon + 1).trim()
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(rawKey)) {
      throw new Error(`unsupported atomic() key: ${rawKey}`)
    }
    let value: string | number
    if (
      (rawValue.startsWith("'") && rawValue.endsWith("'")) ||
      (rawValue.startsWith('"') && rawValue.endsWith('"'))
    ) {
      value = rawValue.slice(1, -1)
    } else if (/^-?\d+(\.\d+)?$/.test(rawValue)) {
      value = Number(rawValue)
    } else {
      throw new Error(`unsupported atomic() value: ${rawValue}`)
    }
    const resolved = resolveDeclaration(activeSchema, rawKey, value)
    for (const declaration of resolved) {
      declarations.push({
        property: declaration.cssProperty,
        value: declaration.value,
      })
    }
  }
  return declarations
}
