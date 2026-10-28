// The composer (spec §8.5): a rounded field that grows to six lines, the +
// menu, and the send button once there is something to send. ChatPane keeps
// it on the keyboard.
import { useCallback, useEffect, useRef, useState, type ComponentRef } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import { Touchable } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';

import {
  cancelEditingMessage,
  editMessage,
  sendMessage,
  sendWithFiles,
} from '@/state/actions/chat';
import { useChannelInfo } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import type { PickedFile } from '@/state/types';
import { Icon, Menu, Text, space, typography, useTheme, type MenuAction } from '@/ui';
import { MAX_FONT_SCALE } from '@/ui/theme/typography';

import { AttachmentChips } from './AttachmentChips';
import { EditingBar } from './EditingBar';
import { useTypingNotifier } from './useTypingNotifier';

export interface ComposerProps {
  serverKey: string;
  channelId: string;
}

const MAX_CHARS = 4000;
const COUNTER_THRESHOLD = 3900;
/** The field, the + and send buttons: Telegram's 40 pt. */
const CONTROL = 40;
const MAX_LINES = 6;
const FIELD_PAD_V = (CONTROL - (typography.body.lineHeight ?? 22)) / 2;
const FIELD_MAX = (typography.body.lineHeight ?? 22) * MAX_LINES + FIELD_PAD_V * 2;

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
  const input = useRef<ComponentRef<typeof TextInput>>(null);

  const currentEditingId = isEditing ? editing.messageId : null;
  if (currentEditingId !== prevEditingId) {
    setPrevEditingId(currentEditingId);
    if (isEditing) {
      setText(editing.text);
    }
  }

  // Starting an edit brings the keyboard up on the text.
  useEffect(() => {
    if (currentEditingId !== null) input.current?.focus();
  }, [currentEditingId]);

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
      <View style={[styles.bar, { backgroundColor: colors.sidebar, borderTopColor: colors.border, paddingBottom: insets.bottom + space.md }]}>
        <Text variant="callout" color="textSecondary" style={styles.cannotSend}>
          {"You can't send messages here."}
        </Text>
      </View>
    );
  }

  const channelName = info?.channel.name ?? '';
  const hasSomething = isEditing || text.trim().length > 0 || files.length > 0;

  return (
    <View style={[styles.bar, { backgroundColor: colors.sidebar, borderTopColor: colors.border, paddingBottom: insets.bottom + space.sm }]}>
      {isEditing && <EditingBar onCancel={handleCancelEditing} />}
      {files.length > 0 && <AttachmentChips files={files} onRemove={handleRemoveFile} />}

      <View style={styles.row}>
        {canAttach && !isEditing && (
          <Menu actions={attachActions} onAction={(id) => report(handleAttachAction(id), 'attach files')} testID="composer-attach">
            <View style={styles.control} accessible accessibilityRole="button" accessibilityLabel="Attach">
              <Icon name="add" size={24} color="textSecondary" />
            </View>
          </Menu>
        )}

        <View style={[styles.field, { backgroundColor: colors.hover }]}>
          <TextInput
            ref={input}
            style={[typography.body, styles.input, { color: colors.text }]}
            placeholder={`Message #${channelName}`}
            placeholderTextColor={colors.textMuted}
            value={text}
            onChangeText={handleChangeText}
            multiline
            maxFontSizeMultiplier={MAX_FONT_SCALE}
            textAlignVertical="center"
            cursorColor={colors.text}
            selectionColor={colors.selected}
            accessibilityLabel={`Message #${channelName}`}
            testID="composer-input"
          />
          {showCounter && (
            <Text
              variant="footnote"
              color={isOverLimit ? 'recording' : 'textMuted'}
              tabular
              style={styles.counter}
              testID="composer-counter"
            >
              {String(remainingChars)}
            </Text>
          )}
        </View>

        {hasSomething && (
          <Touchable
            onPress={() => report(handleSubmit(), 'send')}
            disabled={!canSubmit}
            hitSlop={2}
            style={[styles.control, styles.send, { backgroundColor: colors.accent, opacity: canSubmit ? 1 : 0.4 }]}
          >
            <View
              style={styles.center}
              accessible
              accessibilityRole="button"
              accessibilityLabel={isEditing ? 'Save edit' : 'Send'}
              accessibilityState={{ disabled: !canSubmit }}
              testID="composer-send"
            >
              <Icon name={isEditing ? 'check' : 'send'} size={20} color="onAccent" />
            </View>
          </Touchable>
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    borderTopWidth: StyleSheet.hairlineWidth,
    paddingTop: space.sm,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    gap: space.xs,
    paddingHorizontal: space.sm,
  },
  control: {
    width: CONTROL,
    height: CONTROL,
    borderRadius: CONTROL / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  send: {
    overflow: 'hidden',
  },
  center: {
    flex: 1,
    alignSelf: 'stretch',
    alignItems: 'center',
    justifyContent: 'center',
  },
  field: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'flex-end',
    minHeight: CONTROL,
    borderRadius: CONTROL / 2,
    paddingHorizontal: space.md + 2,
  },
  input: {
    flex: 1,
    maxHeight: FIELD_MAX,
    paddingTop: FIELD_PAD_V,
    paddingBottom: FIELD_PAD_V,
    paddingHorizontal: 0,
    margin: 0,
  },
  counter: {
    marginLeft: space.sm,
    paddingBottom: FIELD_PAD_V + 2,
  },
  cannotSend: {
    textAlign: 'center',
    paddingVertical: space.sm,
  },
});
