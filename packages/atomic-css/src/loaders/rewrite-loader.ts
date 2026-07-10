import type { LoaderDefinitionFunction } from '@rspack/core'
import { rewriteAtomicSource } from '../compiler/rewrite.js'

const rewriteLoader: LoaderDefinitionFunction = function rewriteLoader(source) {
  if (!source.includes('atomic(')) {
    return source
  }
  const { code } = rewriteAtomicSource(source)
  return code
}

export default rewriteLoader
