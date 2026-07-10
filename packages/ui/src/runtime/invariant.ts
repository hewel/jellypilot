export class UIInvariantError extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(message)
    this.name = 'UIInvariantError'
    this.code = code
  }
}

export function uiInvariant(
  condition: unknown,
  code: string,
  message: string,
): asserts condition {
  if (!condition) {
    throw new UIInvariantError(code, message)
  }
}
