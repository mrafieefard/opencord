// A banner across the voice pane while the link isn't connected (spec §8.6).
import type { VoiceLinkState } from '@opencord/core-native';

import { Banner } from '@/ui';

export interface LinkBannerProps {
  link: VoiceLinkState | null | undefined;
  onRejoin?: () => void;
}

export function LinkBanner({ link, onRejoin }: LinkBannerProps) {
  if (!link) return null;

  switch (link.tag) {
    case 'AwaitingEndpoint':
    case 'Authenticating':
    case 'RtcConnecting':
      return <Banner text="Connecting…" tone="neutral" />;
    case 'Reconnecting':
      return <Banner text="Reconnecting…" tone="neutral" />;
    case 'NoRoute':
      return <Banner text="Can't reach the voice server. Your network may block it." tone="error" />;
    case 'Disconnected':
      return (
        <Banner
          text="Disconnected from voice."
          tone="error"
          action={onRejoin ? { title: 'Rejoin', onPress: onRejoin } : undefined}
        />
      );
    case 'Connected':
    default:
      return null;
  }
}
