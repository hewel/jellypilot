export type FamilyRegistryEntry = {
  name: string
  exportName: string
  path: string
  catalogTitle: string
}

/** Single source for public exports, catalog navigation, and drift checks. */
export const familyRegistry = [
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
] as const satisfies readonly FamilyRegistryEntry[]

export type FamilyName = (typeof familyRegistry)[number]['exportName']
