import * as ImagePicker from 'expo-image-picker';

import { filePath } from '@/core/paths';
import { toast } from '@/state/actions/ui';
import type { PickedFile } from '@/state/types';
import { showActionSheet, type ActionSheetAction } from '@/ui';

export interface PickPhotoOptions {
  title: string;
  allowRemove: boolean;
}

export type PickPhotoResult =
  | { kind: 'picked'; file: PickedFile }
  | { kind: 'removed' }
  | { kind: 'cancelled' };

export async function pickPhoto(options: PickPhotoOptions): Promise<PickPhotoResult> {
  return new Promise((resolve) => {
    const actions: ActionSheetAction[] = [
      { id: 'choose', title: 'Choose a photo' },
      { id: 'camera', title: 'Take a photo' },
    ];
    if (options.allowRemove) {
      actions.push({ id: 'remove', title: 'Remove photo', destructive: true });
    }

    showActionSheet({
      title: options.title,
      actions,
      onAction: async (id) => {
        if (id === 'remove') {
          resolve({ kind: 'removed' });
          return;
        }

        if (id === 'choose') {
          try {
            const res = await ImagePicker.launchImageLibraryAsync({
              allowsEditing: true,
              aspect: [1, 1],
              quality: 0.9,
              mediaTypes: ['images'],
            });
            const asset = !res.canceled ? res.assets?.[0] : undefined;
            if (!asset) {
              resolve({ kind: 'cancelled' });
              return;
            }
            const file: PickedFile = {
              path: filePath(asset.uri),
              filename: asset.fileName || asset.uri.split('/').pop() || 'photo.jpg',
              size: asset.fileSize ?? 0,
              mimeType: asset.mimeType ?? 'image/jpeg',
            };
            resolve({ kind: 'picked', file });
          } catch {
            resolve({ kind: 'cancelled' });
          }
          return;
        }

        if (id === 'camera') {
          try {
            const permission = await ImagePicker.requestCameraPermissionsAsync();
            if (!permission.granted) {
              toast("Opencord can't use the camera. Allow it in Settings.");
              resolve({ kind: 'cancelled' });
              return;
            }

            const res = await ImagePicker.launchCameraAsync({
              allowsEditing: true,
              aspect: [1, 1],
              quality: 0.9,
              mediaTypes: ['images'],
            });
            const asset = !res.canceled ? res.assets?.[0] : undefined;
            if (!asset) {
              resolve({ kind: 'cancelled' });
              return;
            }
            const file: PickedFile = {
              path: filePath(asset.uri),
              filename: asset.fileName || asset.uri.split('/').pop() || 'photo.jpg',
              size: asset.fileSize ?? 0,
              mimeType: asset.mimeType ?? 'image/jpeg',
            };
            resolve({ kind: 'picked', file });
          } catch {
            resolve({ kind: 'cancelled' });
          }
          return;
        }

        resolve({ kind: 'cancelled' });
      },
      onCancel: () => {
        resolve({ kind: 'cancelled' });
      },
    });
  });
}
