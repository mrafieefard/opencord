import { FailureKind, MobileError } from '@opencord/core-native';

import { describeError } from '../errors';

const server = (code: string, message = 'the server said so') =>
  new MobileError.Server({ code, message, retryAfterMs: undefined });

describe('error sentences', () => {
  it.each([
    [new MobileError.NotInitialized(), "Opencord couldn't start. Close it and open it again."],
    [new MobileError.NoIdentity(), 'No identity is loaded.'],
    [new MobileError.UnknownServer(), "That server isn't in your list any more."],
    [new MobileError.NotConnected(), 'Not connected to this server right now.'],
    [new MobileError.Timeout(), "The server didn't answer in time."],
    [new MobileError.InvalidInput({ message: 'display name must not be empty' }), 'display name must not be empty'],
    [server('forbidden'), "You don't have permission to do that."],
    [server('not_found'), "That's gone. It may have been deleted."],
    [server('rate_limited'), 'Slow down a little and try again.'],
    [server('voice_channel_full'), 'This channel is full.'],
    [server('file_too_large'), 'That file is bigger than this server allows.'],
    [server('file_type_blocked'), "This server doesn't take that kind of file."],
    [server('storage_full'), "The server can't store files right now."],
    [server('storage_unavailable'), "The server can't store files right now."],
    [server('music_queue_full', 'The queue is full.'), 'The queue is full.'],
    [server('conflict'), 'The server refused that.'],
    [
      new MobileError.FingerprintMismatch({ expected: 'a', presented: 'b' }),
      "This server's certificate changed. If you didn't expect that, don't trust it.",
    ],
    [
      new MobileError.Rejected({ reason: FailureKind.FingerprintChanged, message: '' }),
      "This server's certificate changed. If you didn't expect that, don't trust it.",
    ],
    [new MobileError.Rejected({ reason: FailureKind.Kicked, message: '' }), 'You were kicked from this server.'],
    [new MobileError.Rejected({ reason: FailureKind.Banned, message: '' }), "You're banned from this server."],
    [
      new MobileError.Rejected({ reason: FailureKind.Rejected, message: '' }),
      "This server didn't let you in. The invite may be used up or wrong.",
    ],
    [
      new MobileError.Rejected({ reason: FailureKind.Incompatible, message: '' }),
      'This server runs a different version of Opencord.',
    ],
    [new MobileError.Connection({ message: 'refused' }), "Couldn't connect. Check your connection and try again."],
    [new MobileError.Storage({ message: 'full' }), "Couldn't save on this phone. It may be out of space."],
    [new MobileError.Cancelled(), 'Cancelled.'],
    [new MobileError.Other({ message: 'camera' }), 'Something went wrong.'],
    [new Error('not ours'), 'Something went wrong.'],
  ])('%#: reads as its sentence', (error, sentence) => {
    expect(describeError(error)).toBe(sentence);
  });
});
