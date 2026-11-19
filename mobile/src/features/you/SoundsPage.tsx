// User settings → Sounds (Phase 2 plan §11.7, spec §3 P1, spec §6.3):
// soundboard listening volume and mute, and voice sounds preferences.
import { useState } from 'react';

import { getCore } from '@/core';
import { CUE_GROUPS } from '@/features/voice/voiceCues';
import { setCueGroup, setSoundboard } from '@/state/actions/prefs';
import { usePrefs } from '@/state/selectors';
import { SettingsList, SettingsRow, SettingsSection } from '@/ui';

const percent = (value: number) => `${Math.round(value)} %`;

export function SoundsPage() {
  const { soundboard, cuesOff } = usePrefs();
  const [volumeMoving, setVolumeMoving] = useState<number | null>(null);

  return (
    <SettingsList>
      {/* Soundboard section */}
      <SettingsSection
        title="Soundboard"
        footer="Sounds others play in your voice channel. Previews play at this volume too."
      >
        <SettingsRow
          kind="slider"
          label="Soundboard volume"
          value={volumeMoving ?? soundboard.volume}
          min={0}
          max={100}
          step={1}
          format={percent}
          onChange={(val) => {
            setVolumeMoving(val);
            try {
              getCore().soundboardSetListening(val / 100, soundboard.muted);
            } catch (e) {
              console.warn(`failed to update listening volume: ${e}`);
            }
          }}
          onDone={(val) => {
            setSoundboard({ volume: val });
            setVolumeMoving(null);
          }}
        />
        <SettingsRow
          kind="switch"
          label="Mute all soundboards"
          value={soundboard.muted}
          onChange={(muted) => setSoundboard({ muted })}
        />
      </SettingsSection>

      {/* Voice sounds section */}
      <SettingsSection
        title="Voice sounds"
        footer="Short sounds in voice channels, heard only by you."
      >
        {CUE_GROUPS.filter((group) => group.id !== 'stream').map((group) => (
          <SettingsRow
            key={group.id}
            kind="switch"
            label={group.label}
            value={!cuesOff.includes(group.id)}
            onChange={(on) => setCueGroup(group.id, on)}
          />
        ))}
      </SettingsSection>
    </SettingsList>
  );
}
