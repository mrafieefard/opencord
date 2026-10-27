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
}

export function ContentPane({ serverKey, open, onOpenSidebar }: ContentPaneProps) {
  if (!open) return <NoChannel />;
  // Keyed by channel, so nothing of one channel's pane carries over to the next.
  return open.kind === 'voice' ? (
    <VoicePane key={open.id} serverKey={serverKey} channelId={open.id} onOpenSidebar={onOpenSidebar} />
  ) : (
    <ChatPane key={open.id} serverKey={serverKey} channelId={open.id} onOpenSidebar={onOpenSidebar} />
  );
}
