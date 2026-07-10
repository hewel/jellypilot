import { createMemo, createSignal, For, Show } from 'solid-js'
import {
  Badge,
  Button,
  Card,
  familyRegistry,
  Heading,
  jellypilotTheme,
  Link,
  neutralTheme,
  Text,
  UIRoot,
  VisuallyHidden,
} from '@jellypilot/ui'
import type { ThemePreference } from '@jellypilot/ui'
import { pageStyle, searchStyle, sectionStyle, shellStyle } from './App.css'

function hashFamily(): string | null {
  const hash = globalThis.location?.hash?.replace(/^#/, '') ?? ''
  return hash.length > 0 ? hash : null
}

export function App() {
  const [preset, setPreset] = createSignal<'neutral' | 'jellypilot'>('neutral')
  const [preference, setPreference] = createSignal<ThemePreference>('system')
  const [query, setQuery] = createSignal('')
  const [active, setActive] = createSignal<string | null>(hashFamily())

  const theme = () =>
    preset() === 'jellypilot' ? jellypilotTheme : neutralTheme

  const filtered = createMemo(() => {
    const q = query().trim().toLowerCase()
    if (!q) return [...familyRegistry]
    return familyRegistry.filter((entry) => {
      const haystack = [
        entry.name,
        entry.exportName,
        entry.catalogTitle,
        entry.path,
      ]
        .join(' ')
        .toLowerCase()
      return haystack.includes(q)
    })
  })

  const selected = createMemo(() => {
    const id = active()
    if (!id) return filtered()[0] ?? null
    return familyRegistry.find((entry) => entry.name === id) ?? null
  })

  if (typeof globalThis.addEventListener === 'function') {
    globalThis.addEventListener('hashchange', () => setActive(hashFamily()))
  }

  return (
    <UIRoot preference={preference()} theme={theme()}>
      <div class={shellStyle}>
        <aside class={sectionStyle}>
          <Heading level={1}>UI Catalog</Heading>
          <Text size="sm">Searchable registry of selected v1 families.</Text>
          <VisuallyHidden>Catalog navigation</VisuallyHidden>
          <input
            class={searchStyle}
            type="search"
            placeholder="Search families, aliases, paths"
            value={query()}
            onInput={(event) => setQuery(event.currentTarget.value)}
            aria-label="Search catalog"
          />
          <div class={sectionStyle}>
            <Text size="sm">Preset</Text>
            <Button
              size="sm"
              variant={preset() === 'neutral' ? 'primary' : 'outline'}
              onClick={() => setPreset('neutral')}
            >
              Neutral
            </Button>
            <Button
              size="sm"
              variant={preset() === 'jellypilot' ? 'primary' : 'outline'}
              onClick={() => setPreset('jellypilot')}
            >
              JellyPilot
            </Button>
            <Text size="sm">Mode</Text>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setPreference('system')}
            >
              System
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setPreference('light')}
            >
              Light
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setPreference('dark')}
            >
              Dark
            </Button>
          </div>
          <nav class={sectionStyle} aria-label="Families">
            <For each={filtered()}>
              {(entry) => (
                <Link href={`#${entry.name}`}>{entry.catalogTitle}</Link>
              )}
            </For>
          </nav>
        </aside>
        <main class={pageStyle}>
          <Show when={selected()} fallback={<Text>No matching families.</Text>}>
            {(entry) => (
              <Card>
                <Badge>{entry().exportName}</Badge>
                <Heading level={2}>{entry().catalogTitle}</Heading>
                <Text size="sm">Hash: #{entry().name}</Text>
                <Text size="sm">Source: {entry().path}</Text>
                <Text size="sm">
                  Public import: @jellypilot/ui or @jellypilot/ui/
                  {entry().exportName}
                </Text>
                <Text size="sm">
                  Documented states: default, disabled where applicable, focus,
                  keyboard, and controlled props. Interaction states use real
                  controls rather than force-state APIs.
                </Text>
              </Card>
            )}
          </Show>
        </main>
      </div>
    </UIRoot>
  )
}
