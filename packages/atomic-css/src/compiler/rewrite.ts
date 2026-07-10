import { createHash } from 'node:crypto'
import { extractAtomicCalls } from './extract.js'
import type { AtomicDeclaration } from './extract.js'

export type RewriteResult = {
  code: string
  rules: Array<{ id: string; declarations: AtomicDeclaration[] }>
}

/** Rewrite atomic() calls into style() compositions; collect canonical rules. */
export function rewriteAtomicSource(source: string): RewriteResult {
  const calls = extractAtomicCalls(source)
  if (calls.length === 0) {
    return { code: source, rules: [] }
  }

  let code = source
  const rules: RewriteResult['rules'] = []
  // Rewrite from the end so earlier offsets stay valid.
  for (let i = calls.length - 1; i >= 0; i -= 1) {
    const call = calls[i]!
    const id = canonicalRuleId(call.declarations)
    rules.unshift({ id, declarations: call.declarations })
    const styleObject = Object.fromEntries(
      call.declarations.map((declaration) => [
        declaration.property,
        declaration.value,
      ]),
    )
    const replacement = `style(${JSON.stringify(styleObject)})`
    code = `${code.slice(0, call.start)}${replacement}${code.slice(call.end)}`
  }

  if (!/\bstyle\b/.test(source) && !/from\s+['"]@vanilla-extract\/css['"]/.test(code)) {
    code = `import { style } from '@vanilla-extract/css';\n${code}`
  } else if (
    !/from\s+['"]@vanilla-extract\/css['"]/.test(code) &&
    /\bstyle\(/.test(code)
  ) {
    code = `import { style } from '@vanilla-extract/css';\n${code}`
  }

  return { code, rules }
}

function canonicalRuleId(declarations: AtomicDeclaration[]): string {
  const payload = declarations
    .map((declaration) => `${declaration.property}:${declaration.value}`)
    .sort()
    .join('|')
  return createHash('sha256').update(payload).digest('hex').slice(0, 12)
}
