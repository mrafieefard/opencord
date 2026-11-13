// Stage A's developer screen (plugins M4 plan, Task 6): opens one plugin
// channel through every real layer, wired by hand as Task 7 will wire it
// properly, and shows the area's status. Reached by a deep link:
//   opencord-app://dev/plugin-area?server=<key>&channel=<id>
// Development builds, and a release built with EXPO_PUBLIC_PLUGIN_GATES=1
// for the measurements.
import * as PluginArea from '@opencord/plugin-area';
import { useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { errorKind, getCore } from '@/core';
import type { CoreApi, ViewOutputRecord } from '@/core/api';
import { Row, Switch, Text, space, useTheme } from '@/ui';
import type { ColorTokens, Scheme } from '@/ui/theme/tokens';

const enabled = __DEV__ || process.env.EXPO_PUBLIC_PLUGIN_GATES === '1';

// One pump for the screen's areas: batches by view id, kept until their
// view's id is known.
const listeners = new Map<string, (json: string) => void>();
let unclaimed: ViewOutputRecord[] = [];
let pumping = false;

function startPump(core: CoreApi) {
  if (pumping) return;
  pumping = true;
  PluginArea.destroyAllAreas();
  const generation = core.viewOutputsAttach();
  void (async () => {
    for (;;) {
      const batch = await core.nextViewOutputs(generation, 64);
      if (batch.length === 0) return;
      for (const record of batch) {
        const listener = listeners.get(record.viewId);
        if (listener) listener(record.json);
        else unclaimed.push(record);
      }
    }
  })();
}

function listen(viewId: string, listener: (json: string) => void): () => void {
  listeners.set(viewId, listener);
  const mine = unclaimed.filter((record) => record.viewId === viewId);
  unclaimed = unclaimed.filter((record) => record.viewId !== viewId);
  for (const record of mine) listener(record.json);
  return () => listeners.delete(viewId);
}

function setupJson(colors: ColorTokens, scheme: Scheme, serverKey: string): string {
  return JSON.stringify({
    theme: scheme,
    colors: {
      text: colors.text,
      text_secondary: colors.textSecondary,
      text_muted: colors.textMuted,
      background: colors.chat,
      surface: colors.surface,
      elevated: colors.elevated,
      border: colors.border,
      accent: colors.accent,
      on_accent: colors.onAccent,
      selected: colors.selected,
      hover: colors.hover,
      danger: colors.recording,
    },
    text_scale: 1,
    safe_area: { top: 0, right: 0, bottom: 0, left: 0 },
    reduce_motion: false,
    server_key: serverKey,
    plugin_id: '',
  });
}

let areaCount = 0;

/** One area showing the channel, wired by hand. */
function GateArea({ serverKey, channelId }: { serverKey: string; channelId: string }) {
  const { colors, scheme } = useTheme();
  // Unique across reloads: an old screen's cleanup must never destroy a new area.
  const [areaId] = useState(() => `gate-${Date.now()}-${++areaCount}`);
  const [shown, setShown] = useState(false);
  const [status, setStatus] = useState('creating');

  useEffect(() => {
    const core = getCore();
    startPump(core);
    let viewId: string | null = null;
    let opening = false;
    let closed = false;
    const waiting: string[] = [];
    let stopListening = () => {};
    const inputs = PluginArea.addInputListener((event) => {
      if (event.areaId !== areaId) return;
      if (viewId !== null) {
        core.viewInput(viewId, event.json);
        return;
      }
      const input = JSON.parse(event.json) as { kind: string };
      if (input.kind !== 'context' || opening) {
        waiting.push(event.json);
        return;
      }
      opening = true;
      const { kind: _kind, ...context } = input;
      core.viewOpen(serverKey, channelId, JSON.stringify(context)).then(
        (id) => {
          if (closed) {
            core.viewClose(id);
            return;
          }
          viewId = id;
          stopListening = listen(id, (json) => PluginArea.pushBatch(areaId, json));
          for (const json of waiting.splice(0)) core.viewInput(id, json);
        },
        (error: unknown) => setStatus(`open failed: ${errorKind(error)}`),
      );
    });
    const statuses = PluginArea.addStatusListener((event) => {
      if (event.areaId === areaId) setStatus(event.json);
    });
    const started = Date.now();
    PluginArea.createArea(areaId, setupJson(colors, scheme, serverKey)).then(
      () => {
        console.warn(`plugin-gate start_ms=${Date.now() - started} area=${areaId}`);
        if (!closed) setShown(true);
      },
      (error: unknown) => setStatus(`create failed: ${String(error)}`),
    );
    return () => {
      closed = true;
      inputs.remove();
      statuses.remove();
      stopListening();
      if (viewId !== null) core.viewClose(viewId);
      PluginArea.destroyArea(areaId);
    };
    // The area lives as long as the screen; a theme change isn't followed here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [areaId, serverKey, channelId]);

  return (
    <View style={styles.area}>
      <Text variant="caption" color="textMuted" numberOfLines={2}>
        {status}
      </Text>
      <View style={[styles.surface, { borderColor: colors.border, backgroundColor: colors.chat }]}>
        {shown && <PluginArea.PluginAreaView areaId={areaId} style={styles.fill} />}
      </View>
    </View>
  );
}

/** Blocks the JavaScript thread for 30 ms out of every 50 while on. */
function useBusyJs(on: boolean) {
  useEffect(() => {
    if (!on) return;
    const timer = setInterval(() => {
      const until = Date.now() + 30;
      while (Date.now() < until) {
        // Busy on purpose.
      }
    }, 50);
    return () => clearInterval(timer);
  }, [on]);
}

export default function PluginAreaGate() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const params = useLocalSearchParams<{ server?: string; channel?: string }>();
  const [busy, setBusy] = useState(false);
  const [second, setSecond] = useState(false);
  useBusyJs(busy);
  if (!enabled) return null;
  const serverKey = params.server ?? '';
  const channelId = params.channel ?? '';
  return (
    <View style={{ flex: 1, backgroundColor: colors.sidebar, paddingTop: insets.top }}>
      {/* The keyboard shrinks the areas, so the plugin lays its fields out above it. */}
      <KeyboardAvoidingView behavior="padding" style={styles.fill}>
        <View style={styles.controls}>
          <Text variant="caption" color="textMuted">
            {PluginArea.available ? `${serverKey} · ${channelId}` : 'The plugin area module is not in this build'}
          </Text>
          <Row accessibilityLabel="Busy JS">
            <Text variant="body" style={styles.fill}>Busy JS</Text>
            <Switch value={busy} onChange={setBusy} accessibilityLabel="Busy JS" />
          </Row>
          <Row accessibilityLabel="Second area">
            <Text variant="body" style={styles.fill}>Second area</Text>
            <Switch value={second} onChange={setSecond} accessibilityLabel="Second area" />
          </Row>
        </View>
        {serverKey !== '' && channelId !== '' && PluginArea.available && (
          <>
            <GateArea serverKey={serverKey} channelId={channelId} />
            {second && <GateArea serverKey={serverKey} channelId={channelId} />}
          </>
        )}
      </KeyboardAvoidingView>
    </View>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  controls: { paddingHorizontal: space.lg, gap: space.xs },
  area: { flex: 1, padding: space.sm, gap: space.xs },
  surface: { flex: 1, borderWidth: StyleSheet.hairlineWidth, borderRadius: 12, overflow: 'hidden' },
});
