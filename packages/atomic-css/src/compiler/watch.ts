export type WatchAssets = {
  css: string
  manifest: string
}

export type WatchState = {
  lastGood: WatchAssets | null
  lastDiagnostics: string[]
}

export function createWatchState(): WatchState {
  return { lastGood: null, lastDiagnostics: [] }
}

/** Invalid edit keeps last-good assets; valid edit replaces them once. */
export function applyWatchEdit(
  state: WatchState,
  edit: { ok: true; assets: WatchAssets } | { ok: false; diagnostics: string[] },
): WatchState {
  if (edit.ok) {
    return {
      lastGood: edit.assets,
      lastDiagnostics: [],
    }
  }
  const diagnostics = [...edit.diagnostics].sort()
  return {
    lastGood: state.lastGood,
    lastDiagnostics: diagnostics,
  }
}
