import { cx } from '@styled-system/css';
import { Check, Images } from 'lucide-solid';
import { Show } from 'solid-js';
import * as recipes from '~styles/recipes';

import { SectionCard } from '../ui';
import * as styles from './LibrarySettingsCard.styles';
import * as shared from './shared.styles';

interface LibrarySettingsCardProps {
  imageDiskCacheEnabled: boolean;
  onImageDiskCacheEnabledChange: (enabled: boolean) => void;
}

export default function LibrarySettingsCard(props: LibrarySettingsCardProps) {
  return (
    <SectionCard icon={<Images class={shared.sectionIcon.primary} />} title="Library">
      <button
        type="button"
        role="checkbox"
        aria-label="Image disk cache"
        aria-checked={props.imageDiskCacheEnabled}
        onClick={() => props.onImageDiskCacheEnabledChange(!props.imageDiskCacheEnabled)}
        class={styles.toggle}
      >
        <span
          aria-hidden="true"
          class={cx(recipes.checkboxBox, styles.checkboxOffset)}
          classList={{ [styles.checkboxChecked]: props.imageDiskCacheEnabled }}
        >
          <Show when={props.imageDiskCacheEnabled}>
            <Check class={styles.icon3_5} stroke-width={3} />
          </Show>
        </span>
        <div class={styles.copy}>
          <span class={styles.title}>Image disk cache</span>
          <p class={styles.description}>
            Cache Library artwork locally for faster repeat browsing.
          </p>
        </div>
      </button>
    </SectionCard>
  );
}
