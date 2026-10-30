// The pane beside (or under) the channel list: the open channel's chat or
// voice pane, or "Choose a channel".
import { ChatPane } from '@/features/chat/ChatPane';
import { VoicePane } from '@/features/voice/VoicePane';

import { NoChannel } from './NoChannel';
import type { OpenChannel } from './useServerPanes';

export interface ContentPaneProps {
  serverKey: string;
  open: OpenChannel | null;
  onOpenSidebar?: () => void;
  /** Given, the pane draws the connected bar, which opens channels with it. A tablet's is in the sidebar instead. */
  onOpenChannel?: (channelId: string) => void;
  /** The sidebar is over the pane (a phone's). */
  covered?: boolean;
}

export function ContentPane({ serverKey, open, onOpenSidebar, onOpenChannel, covered = false }: ContentPaneProps) {
  if (!open) return <NoChannel />;
  // Keyed by channel, so nothing of one channel's pane carries over to the next.
  return open.kind === 'voice' ? (
    <VoicePane
      key={open.id}
      serverKey={serverKey}
      channelId={open.id}
      onOpenSidebar={onOpenSidebar}
      onOpenChannel={onOpenChannel}
    />
  ) : (
    <ChatPane
      key={open.id}
      serverKey={serverKey}
      channelId={open.id}
      onOpenSidebar={onOpenSidebar}
      onOpenChannel={onOpenChannel}
      covered={covered}
    />
  );
}
