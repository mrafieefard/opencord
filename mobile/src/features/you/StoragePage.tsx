// Storage and logs settings (§6.2, Task 7): media cache size, clear cache,
// and session logs export for troubleshooting.
import { useCallback, useEffect, useState } from 'react';
import * as Sharing from 'expo-sharing';

import { getCore, type CoreApi } from '@/core';
import { describeError } from '@/core/errors';
import { toast } from '@/state/actions/ui';
import { confirm, SettingsList, SettingsSection, SettingsRow } from '@/ui';

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  let value: number;
  let unit: string;
  if (bytes < 1024 * 1024) {
    value = bytes / 1024;
    unit = 'KB';
  } else if (bytes < 1024 * 1024 * 1024) {
    value = bytes / (1024 * 1024);
    unit = 'MB';
  } else {
    value = bytes / (1024 * 1024 * 1024);
    unit = 'GB';
  }
  if (value >= 100) {
    const fixed = value.toFixed(1);
    const text = fixed.endsWith('.0') ? value.toFixed(0) : fixed;
    return `${text} ${unit}`;
  }
  return `${value.toFixed(1)} ${unit}`;
}

export function StoragePage({ core = getCore() }: { core?: CoreApi }) {
  const [size, setSize] = useState<number | null>(null);
  const [crashed, setCrashed] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    core
      .cacheSize()
      .then((bytes) => {
        if (mounted) setSize(bytes);
      })
      .catch((e: unknown) => {
        if (mounted) {
          const msg = e instanceof Error && e.message ? e.message : describeError(e);
          setError(msg);
        }
      });
    core
      .logsCrashed()
      .then((path) => {
        if (mounted) setCrashed(path);
      })
      .catch((e) => {
        console.warn('failed to check crashed logs', e);
      });
    return () => {
      mounted = false;
    };
  }, [core]);

  const handleClear = useCallback(() => {
    if (size === null || size === 0 || error !== null) return;
    confirm({
      title: 'Clear media cache?',
      message: 'This removes downloaded pictures, thumbnails and voice messages from your device.',
      confirmTitle: 'Clear',
      destructive: true,
      onConfirm: async () => {
        try {
          await core.cacheClear();
          const updated = await core.cacheSize();
          setSize(updated);
          toast('Media cache cleared');
        } catch (e) {
          toast(describeError(e));
        }
      },
    });
  }, [size, error, core]);

  const handleExportCurrent = useCallback(async () => {
    try {
      const path = await core.logsExport();
      await Sharing.shareAsync(path);
    } catch (e) {
      toast(describeError(e));
    }
  }, [core]);

  const handleExportCrashed = useCallback(async () => {
    try {
      if (crashed) {
        await Sharing.shareAsync(crashed);
      }
    } catch (e) {
      toast(describeError(e));
    }
  }, [crashed]);

  return (
    <SettingsList>
      <SettingsSection
        title="Media cache"
        footer="Thumbnails, pictures and voice messages you have seen, kept on this device. Saved files are not in it."
      >
        <SettingsRow
          kind="value"
          label="Size"
          value={error ? error : size !== null ? formatFileSize(size) : 'Calculating…'}
        />
        <SettingsRow
          kind="destructive"
          label="Clear media cache"
          disabled={size === null || size === 0 || error !== null}
          onPress={handleClear}
        />
      </SettingsSection>

      <SettingsSection
        title="Logs"
        footer="Logs never contain passwords, keys, messages or addresses. The last 10 sessions are kept."
      >
        <SettingsRow
          kind="action"
          label="Export this session's log"
          detail="To send when something went wrong"
          onPress={handleExportCurrent}
        />
        {crashed !== null && (
          <SettingsRow
            kind="action"
            label="Export the last session's log"
            detail="It ended unexpectedly"
            onPress={handleExportCrashed}
          />
        )}
      </SettingsSection>
    </SettingsList>
  );
}
