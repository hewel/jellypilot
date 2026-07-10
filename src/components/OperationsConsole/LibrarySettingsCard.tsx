import { CheckboxInput } from '@jellypilot/ui';
import { Images } from 'lucide-solid';

import ConsoleSection from './ConsoleSection';

import * as shared from './shared.css';

interface LibrarySettingsCardProps {
  imageDiskCacheEnabled: boolean;
  onImageDiskCacheEnabledChange: (enabled: boolean) => void;
}

export default function LibrarySettingsCard(props: LibrarySettingsCardProps) {
  return (
    <ConsoleSection icon={<Images class={shared.sectionIcon.primary} />} title="Library">
      <CheckboxInput
        checked={props.imageDiskCacheEnabled}
        label="Image disk cache"
        description="Cache Library artwork locally for faster repeat browsing."
        onCheckedChange={(next) => props.onImageDiskCacheEnabledChange(next === true)}
      />
    </ConsoleSection>
  );
}
