import { useAppearance } from '@components/AppearanceProvider';
import { SectionCard, SegmentedControl } from '@components/ui';
import { Palette } from 'lucide-solid';
import { Show } from 'solid-js';

import * as styles from './AppearanceSettingsCard.styles';
import * as shared from './shared.styles';

const DESIGN_THEME_ITEMS = [
  { label: 'Control Room', value: 'controlRoom' as const },
  { label: 'Braun', value: 'braun' as const },
];

const COLOR_MODE_ITEMS = [
  { label: 'Light', value: 'light' as const },
  { label: 'Dark', value: 'dark' as const },
];

export default function AppearanceSettingsCard() {
  const appearance = useAppearance();

  return (
    <SectionCard icon={<Palette class={shared.sectionIcon.primary} />} title="Appearance">
      <div class={styles.stack}>
        <SegmentedControl
          label="Design Theme"
          items={DESIGN_THEME_ITEMS}
          value={appearance.desired().designTheme}
          onValueChange={appearance.selectDesignTheme}
        />
        <SegmentedControl
          label="Color Mode"
          items={COLOR_MODE_ITEMS}
          value={appearance.desired().colorMode}
          onValueChange={appearance.selectColorMode}
        />
        <Show when={appearance.saving()}>
          <p class={styles.saving} aria-live="polite" role="status">
            <span class={styles.pingDot} />
            Saving appearance…
          </p>
        </Show>
      </div>
    </SectionCard>
  );
}
