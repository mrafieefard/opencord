// The composer (spec §8.5), floating over the messages as Telegram's: a
// round attach button, a pill field that grows to six lines, and the send
// button once there is something to send, else the mic for a voice
// message (held to record; useVoiceRecorder). Each is its own glass (iOS)
// or raised surface (Android). ChatPane keeps it on the keyboard.
import { useCallback, useEffect, useRef, useState, type ComponentRef } from 'react';
import { type LayoutChangeEvent, StyleSheet, Text as NativeText, TextInput, View } from 'react-native';
import Animated, { FadeInDown, interpolate, useAnimatedStyle, useSharedValue, withTiming } from 'react-native-reanimated';
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

import { MicButton } from '@/features/voiceMessages/MicButton';
import { RecorderPill } from '@/features/voiceMessages/RecorderPill';
import { LOCK_DISTANCE, useVoiceRecorder } from '@/features/voiceMessages/useVoiceRecorder';

import { AttachmentChips } from './AttachmentChips';
import { useChatDock, useDockBottom } from './dock';
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
/** A line more or fewer: the field grows or shrinks over this, as Telegram's. */
const GROW_MS = 160;

export function Composer({ serverKey, channelId }: ComposerProps) {
  const { colors } = useTheme();
  const bottom = useDockBottom();
  const { fieldGrowth } = useChatDock();
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

  // The field's height follows its text, eased rather than jumping. An
  // unseen copy of the text, as wide as the field's, says how tall the text
  // is: a field of fixed height doesn't report its content's size on iOS.
  const fieldHeight = useSharedValue(CONTROL);
  const onMirrorLayout = useCallback(
    (event: LayoutChangeEvent) => {
      const next = Math.min(Math.max(Math.ceil(event.nativeEvent.layout.height) + FIELD_PAD_V * 2, CONTROL), FIELD_MAX);
      if (next === fieldHeight.get()) return;
      fieldHeight.set(withTiming(next, { duration: GROW_MS }));
      fieldGrowth?.set(withTiming(next - CONTROL, { duration: GROW_MS }));
    },
    [fieldHeight, fieldGrowth],
  );
  const fieldStyle = useAnimatedStyle(() => ({ height: fieldHeight.get() }));
  // The lock hint rises with the finger and fades as it reaches the lock.
  // Only the shared value goes to the UI thread, not the recorder.
  const { dragY } = recorder;
  const lockStyle = useAnimatedStyle(() => ({
    opacity: interpolate(dragY.get(), [-LOCK_DISTANCE, 0], [0.4, 1], 'clamp'),
    transform: [{ translateY: Math.max(dragY.get() * 0.5, -LOCK_DISTANCE / 2) }],
  }));

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

            {/* A one-line slot: the field grows up out of it, over the
                messages, so the dock's size (and the list) never moves. */}
            <View style={styles.fieldSlot}>
            <Animated.View style={[styles.fieldBox, fieldStyle]}>
            <GlassSurface style={styles.field}>
              {/* One line, cut with "…": a multiline field wraps its own
                  placeholder, which stays for VoiceOver, unseen. */}
              <NativeText
                style={[typography.body, styles.mirror]}
                maxFontSizeMultiplier={MAX_FONT_SCALE}
                onLayout={onMirrorLayout}
                aria-hidden
                accessibilityElementsHidden
                importantForAccessibility="no-hide-descendants"
              >
                {/* A last empty line is measured only with something on it. */}
                {text.endsWith('\n') || text === '' ? `${text} ` : text}
              </NativeText>
              {text === '' && (
                <Text variant="body" color="textMuted" numberOfLines={1} style={styles.placeholder} testID="composer-placeholder">
                  {`Message #${channelName}`}
                </Text>
              )}
              <TextInput
                ref={input}
                style={[typography.body, styles.input, { color: colors.text }]}
                placeholder={`Message #${channelName}`}
                placeholderTextColor="transparent"
                value={text}
                onChangeText={handleChangeText}
                scrollEnabled
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
            </Animated.View>
            </View>
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
              dragX={recorder.dragX}
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
              <Animated.View entering={FadeInDown.duration(200)} style={styles.lock}>
                <Animated.View style={lockStyle}>
                  <GlassSurface style={styles.lockPill} testID="voice-lock-hint">
                    <Icon name="lock" size={14} color="textSecondary" />
                    <Icon name="arrowUp" size={12} color="textSecondary" />
                  </GlassSurface>
                </Animated.View>
              </Animated.View>
            )}
            <GestureDetector gesture={recorder.gesture}>
              <View collapsable={false}>
                <MicButton
                  holding={recorder.mode === 'holding'}
                  cancelArmed={recorder.cancelArmed}
                  dragX={recorder.dragX}
                  dragY={recorder.dragY}
                  size={CONTROL}
                />
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
  fieldSlot: {
    flex: 1,
    height: CONTROL,
  },
  fieldBox: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 0,
  },
  field: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'stretch',
    borderRadius: CONTROL / 2,
    paddingHorizontal: space.md + 2,
    overflow: 'hidden',
  },
  input: {
    flex: 1,
    paddingTop: FIELD_PAD_V,
    paddingBottom: FIELD_PAD_V,
    paddingHorizontal: 0,
    margin: 0,
  },
  mirror: {
    position: 'absolute',
    left: space.md + 2,
    right: space.md + 2,
    top: 0,
    opacity: 0,
  },
  placeholder: {
    position: 'absolute',
    left: space.md + 2,
    right: space.md + 2,
    top: FIELD_PAD_V,
  },
  counter: {
    marginLeft: space.sm,
    alignSelf: 'flex-end',
    paddingBottom: FIELD_PAD_V + 2,
  },
  lock: {
    position: 'absolute',
    // Above the mic once it has swollen.
    bottom: CONTROL + space.lg,
    left: (CONTROL - 32) / 2,
  },
  lockPill: {
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
