// Every core error as a sentence for people (plan Task 4's table). The
// details go to the log; content never does.
import { FailureKind, MobileError, MobileError_Tags } from '@opencord/core-native';

const certificateChanged = "This server's certificate changed. If you didn't expect that, don't trust it.";

export function failureSentence(reason: FailureKind): string {
  switch (reason) {
    case FailureKind.Kicked:
      return 'You were kicked from this server.';
    case FailureKind.Banned:
      return "You're banned from this server.";
    case FailureKind.Rejected:
      return "This server didn't let you in. The invite may be used up or wrong.";
    case FailureKind.Incompatible:
      return 'This server runs a different version of Opencord.';
    case FailureKind.FingerprintChanged:
      return certificateChanged;
  }
}

function serverSentence(code: string, message: string): string {
  if (code.startsWith('music_')) return message;
  switch (code) {
    case 'forbidden':
      return "You don't have permission to do that.";
    case 'not_found':
      return "That's gone. It may have been deleted.";
    case 'rate_limited':
      return 'Slow down a little and try again.';
    case 'voice_channel_full':
      return 'This channel is full.';
    case 'sound_cooldown':
      return 'Wait a moment to play another sound.';
    case 'file_too_large':
      return 'That file is bigger than this server allows.';
    case 'file_type_blocked':
      return "This server doesn't take that kind of file.";
    case 'storage_full':
    case 'storage_unavailable':
      return "The server can't store files right now.";
    default:
      return 'The server refused that.';
  }
}

export function describeError(error: unknown): string {
  if (!MobileError.instanceOf(error)) return 'Something went wrong.';
  switch (error.tag) {
    case MobileError_Tags.NotInitialized:
      return "Opencord couldn't start. Close it and open it again.";
    case MobileError_Tags.NoIdentity:
      return 'No identity is loaded.';
    case MobileError_Tags.UnknownServer:
      return "That server isn't in your list any more.";
    case MobileError_Tags.NotConnected:
      return 'Not connected to this server right now.';
    case MobileError_Tags.Timeout:
      return "The server didn't answer in time.";
    case MobileError_Tags.InvalidInput:
      return error.inner.message;
    case MobileError_Tags.Server:
      return serverSentence(error.inner.code, error.inner.message);
    case MobileError_Tags.FingerprintMismatch:
      return certificateChanged;
    case MobileError_Tags.Rejected:
      return failureSentence(error.inner.reason);
    case MobileError_Tags.Connection:
      return "Couldn't connect. Check your connection and try again.";
    case MobileError_Tags.Storage:
      return "Couldn't save on this phone. It may be out of space.";
    case MobileError_Tags.Cancelled:
      return 'Cancelled.';
    case MobileError_Tags.Other:
      return 'Something went wrong.';
  }
}

/** For logs: the error's kind and code, never its content. */
export function errorKind(error: unknown): string {
  if (!MobileError.instanceOf(error)) return error instanceof Error ? error.name : typeof error;
  return error.tag === MobileError_Tags.Server ? `Server(${error.inner.code})` : error.tag;
}
