import type { ParentProps } from 'solid-js'
import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js'
import { Portal } from 'solid-js/web'
import { ThemeContext } from '../theme/context'
import { neutralTheme } from '../theme/neutral'
import type {
  ThemeDescriptor,
  ThemeMode,
  ThemePreference,
} from '../theme/types'
import { uiInvariant } from '../runtime/invariant'
import {
  LinkAdapterContext,
  type LinkAdapter,
} from '../runtime/link-adapter'
import { LayerProvider } from '../runtime/layers'

const OWNER_KEY = '__jellypilotUIRootOwner'

type OwnerDoc = Document & {
  [OWNER_KEY]?: symbol
}

export type UIRootProps = ParentProps<{
  preference?: ThemePreference
  theme?: ThemeDescriptor
  linkAdapter?: LinkAdapter
  document?: Document
}>

export function UIRoot(props: UIRootProps) {
  const doc = () => props.document ?? globalThis.document
  const ownerToken = Symbol('ui-root')
  const [systemDark, setSystemDark] = createSignal(false)
  const [portalHost, setPortalHost] = createSignal<HTMLElement | null>(null)

  const preference = createMemo<ThemePreference>(
    () => props.preference ?? 'system',
  )
  const descriptor = createMemo(() => props.theme ?? neutralTheme)
  const resolved = createMemo<ThemeMode>(() => {
    const pref = preference()
    if (pref === 'system') return systemDark() ? 'dark' : 'light'
    return pref
  })

  onMount(() => {
    const ownerDoc = doc() as OwnerDoc
    uiInvariant(
      ownerDoc[OWNER_KEY] === undefined,
      'duplicate-uiroot',
      'Exactly one UIRoot may own an owner document',
    )
    ownerDoc[OWNER_KEY] = ownerToken

    const media = ownerDoc.defaultView?.matchMedia?.(
      '(prefers-color-scheme: dark)',
    )
    if (media) {
      setSystemDark(media.matches)
      const onChange = () => setSystemDark(media.matches)
      media.addEventListener('change', onChange)
      onCleanup(() => media.removeEventListener('change', onChange))
    }

    const host = ownerDoc.createElement('div')
    host.dataset.jpPortalHost = ''
    ownerDoc.body.append(host)
    setPortalHost(host)

    onCleanup(() => {
      if (ownerDoc[OWNER_KEY] === ownerToken) {
        delete ownerDoc[OWNER_KEY]
      }
      host.remove()
      delete ownerDoc.documentElement.dataset.theme
      delete ownerDoc.documentElement.dataset.themeId
    })
  })

  createEffect(() => {
    const ownerDoc = doc()
    ownerDoc.documentElement.dataset.theme = resolved()
    ownerDoc.documentElement.dataset.themeId = descriptor().id
  })

  const themeValue = createMemo(() => ({
    preference: preference(),
    resolved: resolved(),
    descriptor: descriptor(),
  }))

  return (
    <LinkAdapterContext.Provider value={props.linkAdapter}>
      <ThemeContext.Provider value={themeValue()}>
        <LayerProvider portalHost={portalHost}>
          <div data-jp-uiroot="" data-theme={resolved()}>
            {props.children}
            {portalHost() ? (
              <Portal mount={portalHost()!}>
                <span data-jp-portal-root="" hidden />
              </Portal>
            ) : null}
          </div>
        </LayerProvider>
      </ThemeContext.Provider>
    </LinkAdapterContext.Provider>
  )
}

export function assertNoUIRoot(documentRef: Document = document): void {
  const ownerDoc = documentRef as OwnerDoc
  uiInvariant(
    ownerDoc[OWNER_KEY] === undefined,
    'duplicate-uiroot',
    'Exactly one UIRoot may own an owner document',
  )
}
