import { ConnectionRecord, FailureKind } from '@opencord/core-native';

import { connectionText } from '../connectionText';

describe('connectionText', () => {
  it('returns null when connected', () => {
    expect(connectionText(new ConnectionRecord.Connected())).toBeNull();
  });

  it('handles Connecting…', () => {
    expect(connectionText(new ConnectionRecord.Connecting())).toEqual({
      text: 'Connecting…',
      error: false,
    });
  });

  it('handles Reconnecting…', () => {
    expect(
      connectionText(new ConnectionRecord.Reconnecting({ attempt: 2, retryInMs: 3000 })),
    ).toEqual({
      text: 'Reconnecting…',
      error: false,
    });
  });

  it('handles each Failed reason as Task 4 sentence with error: true', () => {
    const reasons = [
      {
        reason: FailureKind.Kicked,
        expected: 'You were kicked from this server.',
      },
      {
        reason: FailureKind.Banned,
        expected: "You're banned from this server.",
      },
      {
        reason: FailureKind.Rejected,
        expected: "This server didn't let you in. The invite may be used up or wrong.",
      },
      {
        reason: FailureKind.Incompatible,
        expected: 'This server runs a different version of Opencord.',
      },
      {
        reason: FailureKind.FingerprintChanged,
        expected: "This server's certificate changed. If you didn't expect that, don't trust it.",
      },
    ];

    for (const { reason, expected } of reasons) {
      expect(
        connectionText(new ConnectionRecord.Failed({ reason, message: 'details' })),
      ).toEqual({
        text: expected,
        error: true,
      });
    }
  });
});
