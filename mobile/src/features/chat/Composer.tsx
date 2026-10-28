import { useCallback, useState } from 'react';
import {
  Pressable,
  StyleSheet,
  TextInput,
  View,
} from 'react-native';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import {
  cancelEditingMessage,
  editMessage,
  sendMessage,
  sendWithFiles,
} from '@/state/actions/chat';
import { useChannelInfo } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import type { PickedFile } from '@/state/types';
import { Icon, Menu, Text, space, useTheme, type MenuAction } from '@/ui';

import { AttachmentChips } from './AttachmentChips';
import { EditingBar } from './EditingBar';
import { useTypingNotifier } from './useTypingNotifier';

export interface ComposerProps {
  serverKey: string;
  channelId: string;
}

const MAX_CHARS = 4000;
const COUNTER_THRESHOLD = 3900;

export function Composer({ serverKey, channelId }: ComposerProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const info = useChannelInfo(serverKey, channelId);
  const editing = useAppStore((state) => state.editingMessage);
  const isEditing = editing && editing.serverKey === serverKey && editing.channelId === channelId;

  const [text, setText] = useState(() => (isEditing ? editing.text : ''));
  const [prevEditingId, setPrevEditingId] = useState<string | null>(() => (isEditing ? editing.messageId : null));
  const [files, setFiles] = useState<PickedFile[]>([]);
  const notifyTyping = useTypingNotifier(serverKey, channelId);

  const currentEditingId = isEditing ? editing.messageId : null;
  if (currentEditingId !== prevEditingId) {
    setPrevEditingId(currentEditingId);
    if (isEditing) {
      setText(editing.text);
    }
  }

  const handleCancelEditing = useCallback(() => {
    cancelEditingMessage();
    setText('');
  }, []);

  const handleChangeText = useCallback(
    (nextText: string) => {
      setText(nextText);
      if (nextText.length > 0 && !isEditing) {
        notifyTyping();
      }
    },
    [isEditing, notifyTyping],
  );

  const canSend = info ? info.access.send : true;
  const canAttach = info ? info.access.attach : true;

  const attachActions: MenuAction[] = [
    { id: 'photo', title: 'Photo library', icon: 'photo' },
    { id: 'file', title: 'Files', icon: 'file' },
  ];

  const handleAttachAction = useCallback(async (id: string) => {
    if (id === 'photo') {
      const res = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ImagePicker.MediaTypeOptions.Images,
        allowsMultipleSelection: true,
      });
      if (!res.canceled && res.assets) {
        const picked: PickedFile[] = res.assets.map((asset) => ({
          path: asset.uri,
          filename: asset.fileName || asset.uri.split('/').pop() || 'photo.jpg',
          size: asset.fileSize ?? 0,
          mimeType: asset.mimeType ?? 'image/jpeg',
        }));
        setFiles((prev) => [...prev, ...picked]);
      }
    } else if (id === 'file') {
      const res = await DocumentPicker.getDocumentAsync({
        multiple: true,
      });
      if (!res.canceled && res.assets) {
        const picked: PickedFile[] = res.assets.map((asset) => ({
          path: asset.uri,
          filename: asset.name,
          size: asset.size ?? 0,
          mimeType: asset.mimeType ?? 'application/octet-stream',
        }));
        setFiles((prev) => [...prev, ...picked]);
      }
    }
  }, []);

  const handleRemoveFile = useCallback((index: number) => {
    setFiles((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const remainingChars = MAX_CHARS - text.length;
  const isOverLimit = text.length > MAX_CHARS;
  const showCounter = text.length >= COUNTER_THRESHOLD;

  const canSubmit =
    isEditing
      ? text.trim().length > 0 && !isOverLimit
      : (text.trim().length > 0 || files.length > 0) && !isOverLimit;

  const handleSubmit = useCallback(async () => {
    if (!canSubmit) return;
    const currentText = text.trim();

    if (isEditing) {
      const msgId = editing.messageId;
      cancelEditingMessage();
      setText('');
      await editMessage(serverKey, msgId, currentText);
    } else {
      const toSendText = currentText;
      const toSendFiles = [...files];
      setText('');
      setFiles([]);
      if (toSendFiles.length > 0) {
        await sendWithFiles(serverKey, channelId, toSendText, toSendFiles);
      } else {
        await sendMessage(serverKey, channelId, toSendText);
      }
    }
  }, [canSubmit, text, isEditing, editing, serverKey, channelId, files]);

  if (!canSend) {
    return (
      <View style={[styles.disabledBar, { backgroundColor: colors.chat }]}>
        <Text variant="subhead" color="textSecondary" style={styles.disabledText}>
          {"You can't send messages here."}
        </Text>
      </View>
    );
  }

  const channelName = info?.channel.name ?? '';

  return (
    <KeyboardStickyView offset={{ closed: 0 }}>
      <View
        style={[
          styles.root,
          { backgroundColor: colors.chat, paddingBottom: Math.max(space.xs, insets.bottom) },
        ]}
      >
        {isEditing && <EditingBar onCancel={handleCancelEditing} />}

        {files.length > 0 && (
          <AttachmentChips files={files} onRemove={handleRemoveFile} />
        )}

        <View style={styles.inputRow}>
          {canAttach && !isEditing && (
            <Menu
              actions={attachActions}
              onAction={handleAttachAction}
              testID="composer-attach"
            >
              <View style={[styles.attachBtn, { backgroundColor: colors.surface }]}>
                <Icon name="add" size={20} color="text" />
              </View>
            </Menu>
          )}

          <View style={[styles.inputContainer, { backgroundColor: colors.surface }]}>
            <TextInput
              style={[
                styles.input,
                { color: colors.text },
              ]}
              placeholder={`Message #${channelName}`}
              placeholderTextColor={colors.textMuted}
              value={text}
              onChangeText={handleChangeText}
              multiline
              maxLength={undefined}
              testID="composer-input"
            />

            {showCounter && (
              <Text
                variant="footnote"
                style={[
                  styles.counter,
                  { color: isOverLimit ? colors.recording : colors.textMuted },
                ]}
                testID="composer-counter"
              >
                {String(remainingChars)}
              </Text>
            )}
          </View>

          <Pressable
            onPress={handleSubmit}
            disabled={!canSubmit}
            accessibilityLabel={isEditing ? 'Save edit' : 'Send message'}
            accessibilityRole="button"
            accessibilityState={{ disabled: !canSubmit }}
            testID="composer-send"
            style={[
              styles.sendBtn,
              {
                backgroundColor: canSubmit ? colors.accent : colors.surface,
                opacity: canSubmit ? 1 : 0.5,
              },
            ]}
          >
            <Icon
              name={isEditing ? 'check' : 'send'}
              size={18}
              color={canSubmit ? 'onAccent' : 'textMuted'}
            />
          </Pressable>
        </View>
      </View>
    </KeyboardStickyView>
  );
}

const styles = StyleSheet.create({
  root: {
    paddingHorizontal: space.sm,
    paddingTop: space.xs,
    paddingBottom: space.xs,
  },
  inputRow: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    gap: space.xs,
  },
  attachBtn: {
    width: 36,
    height: 36,
    borderRadius: 18,
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 2,
  },
  inputContainer: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'flex-end',
    borderRadius: 20,
    paddingHorizontal: space.sm,
    paddingVertical: 6,
    minHeight: 38,
    maxHeight: 120,
  },
  input: {
    flex: 1,
    paddingTop: 0,
    paddingBottom: 0,
    fontSize: 16,
    lineHeight: 20,
  },
  counter: {
    marginLeft: space.xs,
    alignSelf: 'center',
    fontWeight: '600',
  },
  sendBtn: {
    width: 36,
    height: 36,
    borderRadius: 18,
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 2,
  },
  disabledBar: {
    paddingVertical: space.md,
    alignItems: 'center',
    justifyContent: 'center',
  },
  disabledText: {
    fontStyle: 'italic',
  },
});
