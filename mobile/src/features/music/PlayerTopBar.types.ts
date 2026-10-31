export interface PlayerTopBarProps {
  channelName: string;
  /** Given while the user isn't in the channel's voice. */
  onJoin?: () => void;
  onClose: () => void;
}
