// One plugin view's session for as long as its pane wants it (plugins M4
// plan, Tasks 11 and 12): made and started by an effect, its host calls
// answered by the phone, closed when the pane goes or asks for a new one.
import * as PluginArea from '@opencord/plugin-area';
import { useCallback, useEffect, useState } from 'react';
import { useWindowDimensions } from 'react-native';

import { getCore } from '@/core';
import type { CoreApi } from '@/core/api';
import { useTheme } from '@/ui';

import { areaSetup } from './areaSetup';
import { answerHostCalls } from './hostCalls';
import { phoneHostDeps } from './phoneHost';
import { startViewPump, type ViewPump } from './viewPump';
import { ViewSession, type ViewPlace } from './viewSession';

let pump: { core: CoreApi; pump: ViewPump } | null = null;

/** The one pump of this JavaScript start, for the core in use. */
export function pluginViewPump(): ViewPump {
  const core = getCore();
  if (pump?.core !== core) {
    pump?.pump.stop();
    pump = { core, pump: startViewPump(core, PluginArea) };
  }
  return pump.pump;
}

export interface PluginSessionOptions {
  /** Null while the view mustn't be open (the channel is unavailable). */
  place: ViewPlace | null;
  pluginId: string;
  /** The server screen's navigation, for the plugin's `open_channel`. */
  openChannel: (channelId: string) => void;
  /** Changing it closes the session and opens a new one (Reload, a channel back). */
  generation: number;
}

export function usePluginSession({ place, pluginId, openChannel, generation }: PluginSessionOptions): ViewSession | null {
  const { colors, scheme } = useTheme();
  const { fontScale } = useWindowDimensions();
  const [session, setSession] = useState<ViewSession | null>(null);
  const placeKey = place ? JSON.stringify(place) : null;

  // Read when a session starts; the PluginArea keeps the setup current after.
  const firstSetup = useCallback(
    (serverKey: string) =>
      areaSetup({
        colors,
        scheme,
        insets: { top: 0, right: 0, bottom: 0, left: 0 },
        textScale: fontScale,
        reduceMotion: false,
        serverKey,
        pluginId,
      }),
    [colors, scheme, fontScale, pluginId],
  );
  const setupRef = useLatest(firstSetup);
  const openRef = useLatest(openChannel);

  useEffect(() => {
    if (placeKey === null) return undefined;
    const where = JSON.parse(placeKey) as ViewPlace;
    const next = new ViewSession({
      core: getCore(),
      pump: pluginViewPump(),
      area: PluginArea,
      place: where,
      setupJson: setupRef.current(where.serverKey),
    });
    const stopAnswering = answerHostCalls(
      next,
      PluginArea,
      phoneHostDeps({ serverKey: where.serverKey, pluginId, openChannel: (id) => openRef.current(id) }),
    );
    void next.start();
    // The session is what this effect makes; the pane draws it.
    setSession(next);
    return () => {
      stopAnswering();
      next.close();
      setSession((current) => (current === next ? null : current));
    };
  }, [placeKey, pluginId, generation, setupRef, openRef]);

  return session;
}

/** A ref that always holds the latest value, set after each render. */
function useLatest<T>(value: T): { readonly current: T } {
  const [ref] = useState(() => ({ current: value }));
  useEffect(() => {
    ref.current = value;
  }, [ref, value]);
  return ref;
}
