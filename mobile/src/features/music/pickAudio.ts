import * as DocumentPicker from 'expo-document-picker';

import { filePath } from '@/core/paths';
import type { PickedFile } from '@/state/types';

export async function pickAudio(): Promise<PickedFile[]> {
  const result = await DocumentPicker.getDocumentAsync({
    type: 'audio/*',
    multiple: true,
    copyToCacheDirectory: true,
  });

  if (result.canceled || !result.assets) return [];

  return result.assets.map((asset) => ({
    filename: asset.name,
    path: filePath(asset.uri),
    mimeType: asset.mimeType ?? 'audio/*',
    size: asset.size ?? 0,
  }));
}
