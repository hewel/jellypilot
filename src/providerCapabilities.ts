import type { MediaServerProvider, ProviderCapabilities } from './bindings';

export const PROVIDER_CAPABILITIES: Record<MediaServerProvider, ProviderCapabilities> = {
  emby: {
    introSkipper: false,
    quickConnect: false,
    remoteControl: true,
    remoteControlAvailable: false,
    remoteControlWarning: null,
  },
  jellyfin: {
    introSkipper: true,
    quickConnect: true,
    remoteControl: true,
    remoteControlAvailable: false,
    remoteControlWarning: null,
  },
};

export function capabilitiesForProvider(provider: MediaServerProvider): ProviderCapabilities {
  return PROVIDER_CAPABILITIES[provider];
}
