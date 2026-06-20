import { getVersion } from '@tauri-apps/api/app';
import { createResource } from 'solid-js';

interface AppVersionProps {
  class?: string;
}

export default function AppVersion(props: AppVersionProps) {
  const [version] = createResource(() => getVersion());
  return (
    <p
      class={
        props.class ??
        'text-on-surface-variant/50 mt-1 font-mono text-[11px] leading-[16px] font-bold tracking-[0.08em] tracking-wider uppercase'
      }
    >
      v{version() ?? '...'}
    </p>
  );
}
