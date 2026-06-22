import AppVersion from '../AppVersion';

interface PageFooterProps {
  appName?: string;
  class?: string;
}

/**
 * Consistent page footer with app name and version.
 */
export default function PageFooter(props: PageFooterProps) {
  return (
    <div class={`py-8 text-center ${props.class ?? ''}`}>
      <p class="text-on-surface-variant/70 text-[12px] leading-[16px]">
        {props.appName ?? 'JellyPilot'}
      </p>
      <AppVersion />
    </div>
  );
}
