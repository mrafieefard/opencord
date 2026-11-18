import { ComponentType, useMemo } from 'react';
import { View, type ViewProps } from 'react-native';

export interface VideoPlayerLike {
  loop: boolean;
  play: () => void;
  pause: () => void;
}

export interface VideoViewPropsLike extends ViewProps {
  player: VideoPlayerLike | null;
  allowsFullscreen?: boolean;
  allowsPictureInPicture?: boolean;
  nativeControls?: boolean;
  startsPictureInPictureAutomatically?: boolean;
  contentFit?: 'contain' | 'cover' | 'fill';
}

let expoVideo: {
  useVideoPlayer: (source: { uri: string } | null, setup?: (player: VideoPlayerLike) => void) => VideoPlayerLike;
  VideoView: ComponentType<VideoViewPropsLike>;
} | null = null;

try {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  expoVideo = require('expo-video');
} catch {
  expoVideo = null;
}

function useFallbackVideoPlayer(source: { uri: string } | null, setup?: (player: VideoPlayerLike) => void): VideoPlayerLike {
  const dummy = useMemo<VideoPlayerLike>(
    () => ({
      loop: false,
      play: () => {},
      pause: () => {},
    }),
    [],
  );
  if (source && setup) {
    setup(dummy);
  }
  return dummy;
}

function FallbackVideoView(props: VideoViewPropsLike) {
  return <View {...props} />;
}

export function useVideoPlayer(
  source: { uri: string } | null,
  setup?: (player: VideoPlayerLike) => void,
): VideoPlayerLike {
  if (expoVideo?.useVideoPlayer) {
    return expoVideo.useVideoPlayer(source, setup);
  }
  // eslint-disable-next-line react-hooks/rules-of-hooks
  return useFallbackVideoPlayer(source, setup);
}

export const VideoView: ComponentType<VideoViewPropsLike> =
  expoVideo?.VideoView ?? FallbackVideoView;
