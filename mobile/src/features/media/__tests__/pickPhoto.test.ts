import * as ImagePicker from 'expo-image-picker';
import { ActionSheetIOS } from 'react-native';

import { useAppStore } from '@/state/store';
import { closeActionSheet, showActionSheet as showAndroidActionSheet } from '@/ui/actionSheet.android';
import { pickPhoto } from '../pickPhoto';

describe('pickPhoto', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    useAppStore.setState({ toasts: [] });
  });

  // Fails if filePath is not used, aspect is not [1, 1], quality is not 0.9,
  // mediaTypes is not ['images'], or the picked file structure is incorrect.
  it('a picked photo comes back as a file', async () => {
    // 1. From photo library
    const spyActionSheet = jest.spyOn(ActionSheetIOS, 'showActionSheetWithOptions');
    spyActionSheet.mockImplementationOnce((options, callback) => {
      // options: ['Cancel', 'Choose a photo', 'Take a photo']
      expect(options.options).toEqual(['Cancel', 'Choose a photo', 'Take a photo']);
      expect(options.cancelButtonIndex).toBe(0);
      callback(1); // Choose a photo
    });

    const spyLibrary = jest.spyOn(ImagePicker, 'launchImageLibraryAsync').mockResolvedValueOnce({
      canceled: false,
      assets: [
        {
          uri: 'file:///data/user/0/dev.opencord/cache/avatar.jpg',
          fileName: 'avatar.jpg',
          fileSize: 4096,
          mimeType: 'image/jpeg',
          width: 500,
          height: 500,
        },
      ],
    });

    const result = await pickPhoto({ title: 'Profile picture', allowRemove: false });

    expect(spyLibrary).toHaveBeenCalledWith({
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.9,
      mediaTypes: ['images'],
    });

    expect(result).toEqual({
      kind: 'picked',
      file: {
        path: '/data/user/0/dev.opencord/cache/avatar.jpg',
        filename: 'avatar.jpg',
        size: 4096,
        mimeType: 'image/jpeg',
      },
    });

    // 2. From camera
    spyActionSheet.mockImplementationOnce((options, callback) => {
      callback(2); // Take a photo
    });

    const spyCameraPerm = jest
      .spyOn(ImagePicker, 'requestCameraPermissionsAsync')
      .mockResolvedValueOnce({ granted: true, status: ImagePicker.PermissionStatus.GRANTED, canAskAgain: true, expires: 'never' });

    const spyCamera = jest.spyOn(ImagePicker, 'launchCameraAsync').mockResolvedValueOnce({
      canceled: false,
      assets: [
        {
          uri: 'file:///data/user/0/dev.opencord/cache/camera.jpg',
          fileName: 'camera.jpg',
          fileSize: 8192,
          mimeType: 'image/jpeg',
          width: 600,
          height: 600,
        },
      ],
    });

    const cameraResult = await pickPhoto({ title: 'Profile picture', allowRemove: false });

    expect(spyCameraPerm).toHaveBeenCalled();
    expect(spyCamera).toHaveBeenCalledWith({
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.9,
      mediaTypes: ['images'],
    });

    expect(cameraResult).toEqual({
      kind: 'picked',
      file: {
        path: '/data/user/0/dev.opencord/cache/camera.jpg',
        filename: 'camera.jpg',
        size: 8192,
        mimeType: 'image/jpeg',
      },
    });
  });

  // Fails if cancelling the sheet, picker or camera resolves to anything other than { kind: 'cancelled' }.
  it('cancelling comes back as cancelled', async () => {
    const spyActionSheet = jest.spyOn(ActionSheetIOS, 'showActionSheetWithOptions');

    // 1. Cancel the action sheet
    spyActionSheet.mockImplementationOnce((options, callback) => {
      callback(0); // Cancel
    });

    const cancelSheetResult = await pickPhoto({ title: 'Profile picture', allowRemove: true });
    expect(cancelSheetResult).toEqual({ kind: 'cancelled' });

    // 2. Cancel library picker
    spyActionSheet.mockImplementationOnce((options, callback) => {
      callback(1); // Choose a photo
    });
    jest.spyOn(ImagePicker, 'launchImageLibraryAsync').mockResolvedValueOnce({
      canceled: true,
      assets: null,
    });

    const cancelLibraryResult = await pickPhoto({ title: 'Profile picture', allowRemove: true });
    expect(cancelLibraryResult).toEqual({ kind: 'cancelled' });

    // 3. Cancel camera
    spyActionSheet.mockImplementationOnce((options, callback) => {
      callback(2); // Take a photo
    });
    jest.spyOn(ImagePicker, 'requestCameraPermissionsAsync').mockResolvedValueOnce({
      granted: true,
      status: ImagePicker.PermissionStatus.GRANTED,
      canAskAgain: true,
      expires: 'never',
    });
    jest.spyOn(ImagePicker, 'launchCameraAsync').mockResolvedValueOnce({
      canceled: true,
      assets: null,
    });

    const cancelCameraResult = await pickPhoto({ title: 'Profile picture', allowRemove: true });
    expect(cancelCameraResult).toEqual({ kind: 'cancelled' });
  });

  // Fails if 'Remove photo' is present when allowRemove is false, or destructive is not set,
  // or selecting remove does not resolve to { kind: 'removed' }.
  it('remove is offered only when allowed', async () => {
    const spyActionSheet = jest.spyOn(ActionSheetIOS, 'showActionSheetWithOptions');

    // When allowRemove is false: no Remove photo option
    let receivedOptions: string[] = [];
    spyActionSheet.mockImplementationOnce((options, callback) => {
      receivedOptions = options.options;
      callback(0);
    });

    await pickPhoto({ title: 'Profile picture', allowRemove: false });
    expect(receivedOptions).toEqual(['Cancel', 'Choose a photo', 'Take a photo']);
    expect(receivedOptions).not.toContain('Remove photo');

    // When allowRemove is true: Remove photo option is present and marked destructive
    let destructiveIndex: number | number[] | undefined;
    spyActionSheet.mockImplementationOnce((options, callback) => {
      receivedOptions = options.options;
      destructiveIndex = options.destructiveButtonIndex;
      callback(3); // Remove photo
    });

    const removeResult = await pickPhoto({ title: 'Profile picture', allowRemove: true });
    expect(receivedOptions).toEqual(['Cancel', 'Choose a photo', 'Take a photo', 'Remove photo']);
    expect(destructiveIndex).toBe(3);
    expect(removeResult).toEqual({ kind: 'removed' });
  });

  // Fails if camera launches without permission, or the toast message is wrong,
  // or result is not { kind: 'cancelled' }.
  it('a refused camera shows the toast', async () => {
    const spyActionSheet = jest.spyOn(ActionSheetIOS, 'showActionSheetWithOptions');
    spyActionSheet.mockImplementationOnce((options, callback) => {
      callback(2); // Take a photo
    });

    jest.spyOn(ImagePicker, 'requestCameraPermissionsAsync').mockResolvedValueOnce({
      granted: false,
      status: ImagePicker.PermissionStatus.DENIED,
      canAskAgain: true,
      expires: 'never',
    });

    const spyCamera = jest.spyOn(ImagePicker, 'launchCameraAsync');

    const result = await pickPhoto({ title: 'Profile picture', allowRemove: false });

    expect(result).toEqual({ kind: 'cancelled' });
    expect(spyCamera).not.toHaveBeenCalled();

    const toasts = useAppStore.getState().toasts;
    expect(toasts.some((t) => t.text === "Opencord can't use the camera. Allow it in Settings.")).toBe(true);
  });

  // Fails if Android action sheet does not trigger the action or cancel on close.
  it('Android action sheet triggers actions and cancel', () => {
    let selected: string | null = null;
    let cancelled = false;
    showAndroidActionSheet({
      title: 'Test',
      actions: [{ id: 'choose', title: 'Choose a photo' }],
      onAction: (id: string) => {
        selected = id;
      },
      onCancel: () => {
        cancelled = true;
      },
    });

    closeActionSheet('choose');
    expect(selected).toBe('choose');

    showAndroidActionSheet({
      title: 'Test',
      actions: [{ id: 'choose', title: 'Choose a photo' }],
      onAction: (id: string) => {
        selected = id;
      },
      onCancel: () => {
        cancelled = true;
      },
    });

    closeActionSheet();
    expect(cancelled).toBe(true);
  });
});
