import { ConnectionRecord, MobileEvent, type MessageRecord } from '@opencord/core-native';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import {
  cancelSend,
  notifyTyping,
  retrySend,
  sendMessage,
  sendWithFiles,
} from '../actions/chat';
import { onEvents, useAppStore } from '../store';
import { channelKey, type PickedFile } from '../types';

function getChannelMessages(key: string) {
  const channelMsgs = useAppStore.getState().messages[key];
  if (!channelMsgs) throw new Error(`Missing channel messages for ${key}`);
  return channelMsgs;
}

describe('sending', () => {
  beforeEach(async () => {
    freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();
    jest.useRealTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('an echo before the send\'s answer shows the message once', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    let resolveSend: (msg: MessageRecord) => void = () => {};
    const sendPromise = new Promise<MessageRecord>((resolve) => {
      resolveSend = resolve;
    });

    jest.spyOn(getCore(), 'messageSend').mockImplementationOnce(() => sendPromise);

    const sendAction = sendMessage(NIGHT_OWLS, CHANNELS.general, 'hello world');

    const pending = getChannelMessages(key).pending;
    expect(pending).toHaveLength(1);
    const nonce = pending[0]?.nonce ?? '';

    const echoMsg: MessageRecord = {
      id: '500',
      channelId: CHANNELS.general,
      authorId: '1000',
      content: 'hello world',
      nonce,
      createdAtMs: Date.now(),
      attachments: [],
    };

    // Echo arrives before messageSend resolves
    onEvents([new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: echoMsg })]);

    expect(getChannelMessages(key).items).toHaveLength(1);
    expect(getChannelMessages(key).items[0]?.id).toBe('500');
    expect(getChannelMessages(key).pending).toHaveLength(0);

    // Now messageSend resolves
    resolveSend(echoMsg);
    await sendAction;

    expect(getChannelMessages(key).items).toHaveLength(1);
    expect(getChannelMessages(key).items[0]?.id).toBe('500');
    expect(getChannelMessages(key).pending).toHaveLength(0);
  });

  it('an echo after the send\'s answer shows the message once', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    let resolveSend: (msg: MessageRecord) => void = () => {};
    const sendPromise = new Promise<MessageRecord>((resolve) => {
      resolveSend = resolve;
    });

    jest.spyOn(getCore(), 'messageSend').mockImplementationOnce(() => sendPromise);

    const sendAction = sendMessage(NIGHT_OWLS, CHANNELS.general, 'hello world');

    const pending = getChannelMessages(key).pending;
    expect(pending).toHaveLength(1);
    const nonce = pending[0]?.nonce ?? '';

    const sentMsg: MessageRecord = {
      id: '501',
      channelId: CHANNELS.general,
      authorId: '1000',
      content: 'hello world',
      nonce,
      createdAtMs: Date.now(),
      attachments: [],
    };

    // messageSend resolves first
    resolveSend(sentMsg);
    await sendAction;

    expect(getChannelMessages(key).items).toHaveLength(1);
    expect(getChannelMessages(key).items[0]?.id).toBe('501');
    expect(getChannelMessages(key).pending).toHaveLength(0);

    // Echo arrives afterwards
    onEvents([new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: sentMsg })]);

    expect(getChannelMessages(key).items).toHaveLength(1);
    expect(getChannelMessages(key).items[0]?.id).toBe('501');
    expect(getChannelMessages(key).pending).toHaveLength(0);
  });

  it('a failed send retried shows once', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    jest.spyOn(getCore(), 'messageSend').mockRejectedValueOnce(new Error('network offline'));

    await sendMessage(NIGHT_OWLS, CHANNELS.general, 'retry message');

    const pending = getChannelMessages(key).pending;
    expect(pending).toHaveLength(1);
    expect(pending[0]?.state).toBe('failed');
    const nonce = pending[0]?.nonce ?? '';

    const successMsg: MessageRecord = {
      id: '502',
      channelId: CHANNELS.general,
      authorId: '1000',
      content: 'retry message',
      nonce,
      createdAtMs: Date.now(),
      attachments: [],
    };

    jest.spyOn(getCore(), 'messageSend').mockResolvedValueOnce(successMsg);

    await retrySend(nonce);

    expect(getChannelMessages(key).items).toHaveLength(1);
    expect(getChannelMessages(key).items[0]?.id).toBe('502');
    expect(getChannelMessages(key).pending).toHaveLength(0);
  });

  it('typing is sent at most every 8 s', async () => {
    jest.useFakeTimers();
    const typingSpy = jest.spyOn(getCore(), 'typingStart').mockResolvedValue();

    // First call sends immediately
    notifyTyping(NIGHT_OWLS, CHANNELS.general);
    expect(typingSpy).toHaveBeenCalledTimes(1);
    expect(typingSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.general);

    // Call after 3s is throttled
    jest.advanceTimersByTime(3000);
    notifyTyping(NIGHT_OWLS, CHANNELS.general);
    expect(typingSpy).toHaveBeenCalledTimes(1);

    // Call after 7s (total 10s > 8s) sends again
    jest.advanceTimersByTime(7000);
    notifyTyping(NIGHT_OWLS, CHANNELS.general);
    expect(typingSpy).toHaveBeenCalledTimes(2);

    // Different channel sends immediately
    notifyTyping(NIGHT_OWLS, CHANNELS.musicRequests);
    expect(typingSpy).toHaveBeenCalledTimes(3);
    expect(typingSpy).toHaveBeenLastCalledWith(NIGHT_OWLS, CHANNELS.musicRequests);

    typingSpy.mockRestore();
  });

  it('cancelling a send stops it and leaves it failed with Cancelled', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    const cancelSpy = jest.spyOn(getCore(), 'sendCancel').mockReturnValue();

    const file: PickedFile = {
      path: 'file:///path/photo.jpg',
      filename: 'photo.jpg',
      size: 1024,
      mimeType: 'image/jpeg',
    };

    let rejectSend: (err: Error) => void = () => {};
    const sendFilesPromise = new Promise<MessageRecord>((_, reject) => {
      rejectSend = reject;
    });

    jest.spyOn(getCore(), 'messageSendFiles').mockImplementationOnce(() => sendFilesPromise);

    const sendPromise = sendWithFiles(NIGHT_OWLS, CHANNELS.general, 'uploading...', [file]);

    const pending = getChannelMessages(key).pending;
    expect(pending).toHaveLength(1);
    const nonce = pending[0]?.nonce ?? '';

    cancelSend(nonce);

    expect(cancelSpy).toHaveBeenCalledWith(NIGHT_OWLS, nonce);

    rejectSend(new Error('Cancelled'));
    await sendPromise.catch(() => {});

    const updatedPending = getChannelMessages(key).pending;
    expect(updatedPending).toHaveLength(1);
    expect(updatedPending[0]?.state).toBe('failed');
    expect(updatedPending[0]?.error).toBe('Cancelled');

    cancelSpy.mockRestore();
  });
});
