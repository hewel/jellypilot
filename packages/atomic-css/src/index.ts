/** Compile-time marker. Must be rewritten by pluginAtomic(). */
export function atomic(_input?: unknown): string {
  throw new Error('atomic() must be compiled by pluginAtomic()')
}
