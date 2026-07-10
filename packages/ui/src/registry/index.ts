export type FamilyRegistryEntry = {
  name: string
  exportName: string
  path: string
  catalogTitle: string
}

/** Single source for public exports, catalog navigation, and drift checks. */
export const familyRegistry = [
  {
    name: 'uiroot',
    exportName: 'UIRoot',
    path: './components/UIRoot.tsx',
    catalogTitle: 'UIRoot',
  },
  {
    name: 'theme',
    exportName: 'Theme',
    path: './components/Theme.tsx',
    catalogTitle: 'Theme',
  },
  {
    name: 'link',
    exportName: 'Link',
    path: './components/Link.tsx',
    catalogTitle: 'Link',
  },
  {
    name: 'text',
    exportName: 'Text',
    path: './components/Text.tsx',
    catalogTitle: 'Text',
  },
  {
    name: 'heading',
    exportName: 'Heading',
    path: './components/Heading.tsx',
    catalogTitle: 'Heading',
  },
  {
    name: 'visually-hidden',
    exportName: 'VisuallyHidden',
    path: './components/VisuallyHidden.tsx',
    catalogTitle: 'VisuallyHidden',
  },
  {
    name: 'button',
    exportName: 'Button',
    path: './components/Button.tsx',
    catalogTitle: 'Button',
  },
  {
    name: 'icon-button',
    exportName: 'IconButton',
    path: './components/IconButton.tsx',
    catalogTitle: 'IconButton',
  },
  {
    name: 'toggle-button',
    exportName: 'ToggleButton',
    path: './components/ToggleButton.tsx',
    catalogTitle: 'ToggleButton',
  },
] as const satisfies readonly FamilyRegistryEntry[]
export type FamilyName = (typeof familyRegistry)[number]['exportName']
