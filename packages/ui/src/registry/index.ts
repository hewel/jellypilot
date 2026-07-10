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
  {
    name: 'card',
    exportName: 'Card',
    path: './components/Card.tsx',
    catalogTitle: 'Card',
  },
  {
    name: 'badge',
    exportName: 'Badge',
    path: './components/Badge.tsx',
    catalogTitle: 'Badge',
  },
  {
    name: 'spinner',
    exportName: 'Spinner',
    path: './components/Spinner.tsx',
    catalogTitle: 'Spinner',
  },
  {
    name: 'skeleton',
    exportName: 'Skeleton',
    path: './components/Skeleton.tsx',
    catalogTitle: 'Skeleton',
  },
  {
    name: 'progress-bar',
    exportName: 'ProgressBar',
    path: './components/ProgressBar.tsx',
    catalogTitle: 'ProgressBar',
  },
  {
    name: 'text-input',
    exportName: 'TextInput',
    path: './components/TextInput.tsx',
    catalogTitle: 'TextInput',
  },
  {
    name: 'text-area',
    exportName: 'TextArea',
    path: './components/TextArea.tsx',
    catalogTitle: 'TextArea',
  },
  {
    name: 'checkbox-input',
    exportName: 'CheckboxInput',
    path: './components/CheckboxInput.tsx',
    catalogTitle: 'CheckboxInput',
  },
  {
    name: 'switch',
    exportName: 'Switch',
    path: './components/Switch.tsx',
    catalogTitle: 'Switch',
  },
  {
    name: 'slider',
    exportName: 'Slider',
    path: './components/Slider.tsx',
    catalogTitle: 'Slider',
  },
  {
    name: 'segmented-control',
    exportName: 'SegmentedControl',
    path: './components/SegmentedControl.tsx',
    catalogTitle: 'SegmentedControl',
  },
  {
    name: 'tabs',
    exportName: 'Tabs',
    path: './components/Tabs.tsx',
    catalogTitle: 'Tabs',
  },
  {
    name: 'collapsible',
    exportName: 'Collapsible',
    path: './components/Collapsible.tsx',
    catalogTitle: 'Collapsible',
  },
  {
    name: 'dialog',
    exportName: 'Dialog',
    path: './components/Dialog.tsx',
    catalogTitle: 'Dialog',
  },
  {
    name: 'alert-dialog',
    exportName: 'AlertDialog',
    path: './components/AlertDialog.tsx',
    catalogTitle: 'AlertDialog',
  },
  {
    name: 'popover',
    exportName: 'Popover',
    path: './components/Popover.tsx',
    catalogTitle: 'Popover',
  },
  {
    name: 'selector',
    exportName: 'Selector',
    path: './components/Selector.tsx',
    catalogTitle: 'Selector',
  },
  {
    name: 'menu',
    exportName: 'Menu',
    path: './components/Menu.tsx',
    catalogTitle: 'Menu',
  },
  {
    name: 'tooltip',
    exportName: 'Tooltip',
    path: './components/Tooltip.tsx',
    catalogTitle: 'Tooltip',
  },
  {
    name: 'hover-card',
    exportName: 'HoverCard',
    path: './components/HoverCard.tsx',
    catalogTitle: 'HoverCard',
  },
  {
    name: 'toast',
    exportName: 'ToastProvider',
    path: './components/Toast.tsx',
    catalogTitle: 'Toast',
  },
] as const satisfies readonly FamilyRegistryEntry[]
export type FamilyName = (typeof familyRegistry)[number]['exportName']
