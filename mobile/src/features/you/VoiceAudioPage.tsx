// Voice & audio on the You tab (phone P1), with the desktop's words
// (app_pages.dart VoicePage, mic_test_panel.dart): noise suppression, the
// input and output volumes, and the microphone test with its level. The
// test runs only while the page is in front: leaving it (another tab, back,
// closing), the app going to the background, or joining a call ends it.
import { NoiseSuppression } from '@opencord/core-native';
import { useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { report } from '@/core';
import { startMicTest, stopMicTest } from '@/state/actions/audio';
import { setAudio } from '@/state/actions/prefs';
import { useAppStore } from '@/state/store';
import { Button, SettingsList, SettingsRow, SettingsSection, space } from '@/ui';

import { LevelMeter } from './LevelMeter';

const SUPPRESSION: { mode: NoiseSuppression; label: string; detail: string }[] = [
  { mode: NoiseSuppression.Off, label: 'Off', detail: 'Sends the microphone as it is' },
  { mode: NoiseSuppression.Standard, label: 'Standard', detail: 'Removes steady noise; very light' },
  { mode: NoiseSuppression.High, label: 'High', detail: 'Also keyboards and dogs; heavier' },
];

const TEST_FOOTER = 'Hear what Opencord sends. Use headphones, or you may hear an echo.';
const IN_CALL_FOOTER = 'Leave the call to test your microphone.';

const percent = (value: number) => `${Math.round(value)} %`;

export function VoiceAudioPage() {
  const audio = useAppStore((state) => state.prefs.audio);
  const testing = useAppStore((state) => state.micTest);
  const inCall = useAppStore((state) => state.myVoice !== null);
  const appActive = useAppStore((state) => state.appActive);
  // A volume while the finger is on its slider, in percent; the store's otherwise.
  const [inputMoving, setInputMoving] = useState<number | null>(null);
  const [outputMoving, setOutputMoving] = useState<number | null>(null);

  // Leaving the page (another tab keeps it mounted, so focus, not mount).
  useFocusEffect(
    useCallback(
      () => () => {
        report(stopMicTest(), 'stop the microphone test');
      },
      [],
    ),
  );

  // The app went to the background.
  useEffect(() => {
    if (!appActive) report(stopMicTest(), 'stop the microphone test');
  }, [appActive]);

  const toggleTest = useCallback(() => {
    if (useAppStore.getState().micTest) report(stopMicTest(), 'stop the microphone test');
    else report(startMicTest(), 'start the microphone test');
  }, []);

  return (
    <SettingsList>
      <SettingsSection title="Noise suppression">
        {SUPPRESSION.map((option) => (
          <SettingsRow
            key={option.label}
            kind="check"
            label={option.label}
            detail={option.detail}
            checked={audio.noiseSuppression === option.mode}
            onPress={() => setAudio({ noiseSuppression: option.mode })}
          />
        ))}
      </SettingsSection>
      <SettingsSection title="Volume">
        <SettingsRow
          kind="slider"
          label="Input volume"
          value={inputMoving ?? audio.inputVolume * 100}
          min={0}
          max={200}
          step={1}
          format={percent}
          onChange={setInputMoving}
          onDone={(value) => {
            setAudio({ inputVolume: value / 100 });
            setInputMoving(null);
          }}
        />
        <SettingsRow
          kind="slider"
          label="Output volume"
          value={outputMoving ?? audio.outputVolume * 100}
          min={0}
          max={200}
          step={1}
          format={percent}
          onChange={setOutputMoving}
          onDone={(value) => {
            setAudio({ outputVolume: value / 100 });
            setOutputMoving(null);
          }}
        />
      </SettingsSection>
      <SettingsSection title="Microphone test" footer={inCall ? IN_CALL_FOOTER : TEST_FOOTER}>
        <View style={styles.test}>
          <Button
            title={testing ? 'Stop test' : 'Test'}
            variant="secondary"
            disabled={inCall}
            onPress={toggleTest}
          />
          {testing && <LevelMeter />}
        </View>
      </SettingsSection>
    </SettingsList>
  );
}

const styles = StyleSheet.create({
  test: { padding: space.lg, gap: space.md },
});
