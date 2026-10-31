// Android has no output button yet: the call follows the system's route.
import type { AudioRouteButtonProps } from './AudioRouteButton.types';

export function AudioRouteButton(_props: AudioRouteButtonProps) {
  return null;
}

/** Whether this platform has the button. */
export const hasAudioRouteButton = false;
