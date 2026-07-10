import { For } from 'solid-js'
import { familyRegistry, Heading, Text, VisuallyHidden } from '@jellypilot/ui'
import { pageStyle, sectionStyle } from './App.css'

export function App() {
  return (
    <main class={pageStyle}>
      <Heading level={1}>JellyPilot UI Catalog</Heading>
      <Text size="md">
        Registry-driven Neutral foundations. Families:
      </Text>
      <VisuallyHidden>Accessible catalog landmark</VisuallyHidden>
      <ul class={sectionStyle}>
        <For each={[...familyRegistry]}>
          {(entry) => (
            <li data-family={entry.name}>
              <Heading level={2}>{entry.catalogTitle}</Heading>
              <Text size="sm">Export: {entry.exportName}</Text>
            </li>
          )}
        </For>
      </ul>
    </main>
  )
}
