// The composer (spec §8.5), floating over the messages as Telegram's: a
// round attach button, a pill field that grows to six lines, and the send
// button once there is something to send, else the mic for a voice
// message (held to record; useVoiceRecorder). Each is its own glass (iOS)
// or raised surface (Android). ChatPane keeps it on the keyboard.
import { useCallback, useEffect, useRef, useState, type ComponentRef } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import { GestureDetector, Touchable } from 'react-native-gesture-handler';

import { report } from '@/core';
import { filePath } from '@/core/paths';

import {
  cancelEditingMessage,
  editMessage,
  sendMessage,
  sendWithFiles,
} from '@/state/actions/chat';
import { useChannelInfo } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import type { PickedFile } from '@/state/types';
import { GlassSurface, Icon, Menu, Text, space, typography, useTheme, type MenuAction } from '@/ui';
import { MAX_FONT_SCALE } from '@/ui/theme/typography';

import { RecorderPill } from '@/features/voiceMessages/RecorderPill';
import { useVoiceRecorder } from '@/features/voiceMessages/useVoiceRecorder';

import { AttachmentChips } from './AttachmentChips';
import { useDockBottom } from './dock';
import { EditingBar } from './EditingBar';
import { useTypingNotifier } from './useTypingNotifier';

export interface ComposerProps {
  serverKey: string;
  channelId: string;
}

const MAX_CHARS = 4000;
const COUNTER_THRESHOLD = 3900;
/** The field and the round buttons: Telegram's floating 44 pt. */
const CONTROL = 44;
const MAX_LINES = 6;
const FIELD_PAD_V = (CONTROL - (typography.body.lineHeight ?? 22)) / 2;
const FIELD_MAX = (typography.body.lineHeight ?? 22) * MAX_LINES + FIELD_PAD_V * 2;

export function Composer({ serverKey, channelId }: ComposerProps) {
  const { colors } = useTheme();
  const bottom = useDockBottom();
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
  const canVoice = info ? info.access.sendVoiceMessages : true;
  const recorder = useVoiceRecorder(serverKey, channelId);

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
          path: filePath(asset.uri),
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
          path: filePath(asset.uri),
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
      <View style={[styles.dock, { paddingBottom: bottom }]} pointerEvents="box-none">
        <GlassSurface style={styles.notice}>
          <Text variant="callout" color="textSecondary" style={styles.cannotSend}>
            {"You can't send messages here."}
          </Text>
        </GlassSurface>
      </View>
    );
  }

  const channelName = info?.channel.name ?? '';
  const hasSomething = isEditing || text.trim().length > 0 || files.length > 0;

  return (
    <View style={[styles.dock, { paddingBottom: bottom }]} pointerEvents="box-none">
      {(isEditing || files.length > 0) && (
        <GlassSurface style={styles.card}>
          {isEditing && <EditingBar onCancel={handleCancelEditing} />}
          {files.length > 0 && <AttachmentChips files={files} onRemove={handleRemoveFile} />}
        </GlassSurface>
      )}

      <View style={styles.row}>
        {/* Left of the mic: the attach button and the field, or while
            recording the recorder. The mic stays the same view throughout,
            so the hold that started recording isn't cut off. */}
        {recorder.mode === 'idle' ? (
          <>
            {canAttach && !isEditing && (
              <GlassSurface interactive style={styles.round}>
                <Menu actions={attachActions} onAction={(id) => report(handleAttachAction(id), 'attach files')} testID="composer-attach">
                  <View style={styles.control} accessible accessibilityRole="button" accessibilityLabel="Attach">
                    <Icon name="attach" size={22} color="text" />
                  </View>
                </Menu>
              </GlassSurface>
            )}

            <GlassSurface style={styles.field}>
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
            </GlassSurface>
          </>
        ) : (
          <>
            {recorder.mode !== 'holding' && (
              <GlassSurface interactive style={styles.round}>
                <Touchable onPress={recorder.remove} style={styles.control}>
                  <View style={styles.center} accessible accessibilityRole="button" accessibilityLabel="Delete voice message">
                    <Icon name="delete" size={20} color="recording" />
                  </View>
                </Touchable>
              </GlassSurface>
            )}
            <RecorderPill
              mode={recorder.mode}
              cancelArmed={recorder.cancelArmed}
              draft={recorder.draft}
              onStop={recorder.stop}
              height={CONTROL}
            />
          </>
        )}

        {recorder.mode === 'locked' || recorder.mode === 'draft' ? (
          <SendButton label="Send voice message" onPress={recorder.send} testID="composer-send-voice" />
        ) : hasSomething && recorder.mode === 'idle' ? (
          <SendButton
            label={isEditing ? 'Save edit' : 'Send'}
            icon={isEditing ? 'check' : 'send'}
            disabled={!canSubmit}
            onPress={() => report(handleSubmit(), 'send')}
            testID="composer-send"
          />
        ) : canVoice && !isEditing ? (
          <View>
            {recorder.mode === 'holding' && (
              <GlassSurface style={styles.lock} testID="voice-lock-hint">
                <Icon name="lock" size={14} color="textSecondary" />
                <Icon name="arrowUp" size={12} color="textSecondary" />
              </GlassSurface>
            )}
            <GestureDetector gesture={recorder.gesture}>
              <View collapsable={false}>
                {recorder.mode === 'holding' ? (
                  <View style={[styles.control, { backgroundColor: recorder.cancelArmed ? colors.recording : colors.voice }]}>
                    <Icon name="mic" size={22} color="onAccent" />
                  </View>
                ) : (
                  <GlassSurface interactive style={styles.round}>
                    <View
                      style={styles.control}
                      accessible
                      accessibilityRole="button"
                      accessibilityLabel="Record a voice message"
                      accessibilityHint="Hold to record, let go to send"
                      testID="composer-mic"
                    >
                      <Icon name="mic" size={22} color="text" />
                    </View>
                  </GlassSurface>
                )}
              </View>
            </GestureDetector>
          </View>
        ) : null}
      </View>
    </View>
  );
}

interface SendButtonProps {
  label: string;
  icon?: 'send' | 'check';
  disabled?: boolean;
  onPress: () => void;
  testID: string;
}

function SendButton({ label, icon = 'send', disabled = false, onPress, testID }: SendButtonProps) {
  const { colors } = useTheme();
  return (
    <Touchable
      onPress={onPress}
      disabled={disabled}
      hitSlop={2}
      style={[styles.control, styles.send, { backgroundColor: colors.accent, opacity: disabled ? 0.4 : 1 }]}
    >
      <View style={styles.center} accessible accessibilityRole="button" accessibilityLabel={label} accessibilityState={{ disabled }} testID={testID}>
        <Icon name={icon} size={20} color="onAccent" />
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  dock: {
    paddingHorizontal: space.sm,
    gap: space.sm,
  },
  card: {
    borderRadius: CONTROL / 2,
    paddingVertical: space.xs,
    overflow: 'hidden',
  },
  row: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    gap: space.sm,
  },
  round: {
    width: CONTROL,
    height: CONTROL,
    borderRadius: CONTROL / 2,
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
  lock: {
    position: 'absolute',
    bottom: CONTROL + space.sm,
    left: (CONTROL - 32) / 2,
    width: 32,
    height: 60,
    borderRadius: 16,
    alignItems: 'center',
    justifyContent: 'center',
    gap: space.xs,
  },
  notice: {
    borderRadius: CONTROL / 2,
    minHeight: CONTROL,
    justifyContent: 'center',
  },
  cannotSend: {
    textAlign: 'center',
    paddingVertical: space.sm,
  },
});
