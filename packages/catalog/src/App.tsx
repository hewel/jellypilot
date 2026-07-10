import { createSignal, For } from 'solid-js'
import {
  familyRegistry,
  Heading,
  jellypilotTheme,
  Link,
  neutralTheme,
  Text,
  Theme,
  UIRoot,
  VisuallyHidden,
} from '@jellypilot/ui'
import { pageStyle, sectionStyle } from './App.css'

export function App() {
  const [preset, setPreset] = createSignal<'neutral' | 'jellypilot'>('neutral')
  const [mode, setMode] = createSignal<'light' | 'dark'>('light')
  const theme = () =>
    preset() === 'jellypilot' ? jellypilotTheme : neutralTheme

  return (
    <UIRoot preference={mode()} theme={theme()}>
      <main class={pageStyle}>
        <Heading level={1}>JellyPilot UI Catalog</Heading>
        <Text size="md">Registry-driven foundations with UIRoot theme control.</Text>
        <VisuallyHidden>Accessible catalog landmark</VisuallyHidden>
        <div class={sectionStyle}>
          <button type="button" onClick={() => setPreset('neutral')}>
            Neutral
          </button>
          <button type="button" onClick={() => setPreset('jellypilot')}>
            JellyPilot
          </button>
          <button type="button" onClick={() => setMode('light')}>
            Light
          </button>
          <button type="button" onClick={() => setMode('dark')}>
            Dark
          </button>
          <Link href="#text">Jump to Text</Link>
        </div>
        <Theme descriptor={theme()} mode={mode()}>
          <ul class={sectionStyle}>
            <For each={[...familyRegistry]}>
              {(entry) => (
                <li data-family={entry.name} id={entry.name}>
                  <Heading level={2}>{entry.catalogTitle}</Heading>
                  <Text size="sm">Export: {entry.exportName}</Text>
                </li>
              )}
            </For>
          </ul>
        </Theme>
      </main>
    </UIRoot>
  )
}
