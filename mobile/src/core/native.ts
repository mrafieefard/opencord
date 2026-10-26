// The real core: the generated bindings, as they are. They throw the
// generated MobileError, which describeError reads.
import * as native from '@opencord/core-native';

import type { CoreApi } from './api';

export const NativeCore: CoreApi = {
  coreInit: (dataDir, cacheDir) => native.coreInit(dataDir, cacheDir),
  coreVersion: () => native.coreVersion(),
  eventsAttach: () => native.eventsAttach(),
  nextEvents: (generation, max) => native.nextEvents(generation, max),
  settingsGet: (key) => native.settingsGet(key),
  settingsSet: (key, value) => native.settingsSet(key, value),

  identityGenerate: () => native.identityGenerate(),
  identityCheck: (secret, name) => native.identityCheck(secret, name),
  identityLoad: (secret, name) => native.identityLoad(secret, name),
  identityBackupEncode: (secret) => native.identityBackupEncode(secret),
  identityBackupDecode: (text) => native.identityBackupDecode(text),

  serversList: () => native.serversList(),
  serverAdd: (link, claim) => native.serverAdd(link, claim),
  serverTrust: (address, fingerprint) => native.serverTrust(address, fingerprint),
  serverRetryNow: (key) => native.serverRetryNow(key),
  serverRemove: (key) => native.serverRemove(key),
  profileUpdate: (key, name) => native.profileUpdate(key, name),
  avatarFile: (key, hash) => native.avatarFile(key, hash),

  messagesFetch: (key, channel, before, limit) => native.messagesFetch(key, channel, before, limit),
  messageSend: (key, channel, content, nonce) => native.messageSend(key, channel, content, nonce),
  messageSendFiles: (key, channel, content, nonce, files) =>
    native.messageSendFiles(key, channel, content, nonce, files),
  sendCancel: (key, nonce) => native.sendCancel(key, nonce),
  messageEdit: (key, message, content) => native.messageEdit(key, message, content),
  messageDelete: (key, message) => native.messageDelete(key, message),
  typingStart: (key, channel) => native.typingStart(key, channel),
  attachmentFile: (key, attachment, variant) => native.attachmentFile(key, attachment, variant),

  voiceJoin: (key, channel) => native.voiceJoin(key, channel),
  voiceLeave: () => native.voiceLeave(),
  voiceSetMute: (muted) => native.voiceSetMute(muted),
  voiceSetDeaf: (deafened) => native.voiceSetDeaf(deafened),
  voiceSetUserVolume: (key, user, volume) => native.voiceSetUserVolume(key, user, volume),
  voiceSetUserLocalMute: (key, user, muted) => native.voiceSetUserLocalMute(key, user, muted),

  musicSettings: (key) => native.musicSettings(key),
  musicSubscribe: (key, channel) => native.musicSubscribe(key, channel),
  musicUnsubscribe: (key, channel) => native.musicUnsubscribe(key, channel),
  musicSearch: (key, channel, query, source) => native.musicSearch(key, channel, query, source),
  musicEnqueue: (key, channel, results, history, before, placement) =>
    native.musicEnqueue(key, channel, results, history, before, placement),
  musicPlayback: (key, channel, action, position, expected) =>
    native.musicPlayback(key, channel, action, position, expected),
  musicSetMode: (key, channel, repeat, shuffle) => native.musicSetMode(key, channel, repeat, shuffle),
  musicQueueMove: (key, channel, item, before) => native.musicQueueMove(key, channel, item, before),
  musicQueueRemove: (key, channel, items) => native.musicQueueRemove(key, channel, items),
  musicQueueClear: (key, channel) => native.musicQueueClear(key, channel),
  musicVoteSkip: (key, channel, expected) => native.musicVoteSkip(key, channel, expected),
  musicSetListening: (volume, muted, duck) => native.musicSetListening(volume, muted, duck),
  musicArtwork: (key, artwork, size) => native.musicArtwork(key, artwork, size),
  musicUploadSong: (key, channel, upload, path, playNow) =>
    native.musicUploadSong(key, channel, upload, path, playNow),
  musicCancelSong: (key, upload) => native.musicCancelSong(key, upload),
};
