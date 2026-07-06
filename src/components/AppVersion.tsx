import { createQuery } from '@tanstack/solid-query';
import { getVersion } from '@tauri-apps/api/app';

import { queryKeys } from '../effects/query';

import * as styles from './AppVersion.css';

interface AppVersionProps {
  class?: string;
}

export default function AppVersion(props: AppVersionProps) {
  const versionQuery = createQuery(() => ({
    queryKey: queryKeys.appVersion,
    queryFn: getVersion,
    staleTime: Infinity,
  }));
  return <p class={props.class ?? styles.version}>v{versionQuery.data ?? '...'}</p>;
}
