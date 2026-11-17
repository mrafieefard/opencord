import { MobileError, MobileEvent, PresenceStatus } from '@opencord/core-native';

import { NIGHT_OWLS, SELF_ID, snapshot, user } from '@/core/fake/world';
import { catchUpAvatar, catchUpPresence, setAvatar, setStatus } from '@/state/actions/presence';
import { onEvents, setReadyHook, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

describe('presence and avatar catch-up', () => {
  beforeEach(() => {
    freshStore();
    setReadyHook((serverKey) => {
      catchUpPresence(serverKey);
      catchUpAvatar(serverKey);
    });
  });

  // Fails if status is not sent again after a server's Ready event.
  it('status is sent again after Ready', async () => {
    const core = freshStore();
    setReadyHook((serverKey) => {
      catchUpPresence(serverKey);
      catchUpAvatar(serverKey);
    });

    await setStatus('idle');
    expect(useAppStore.getState().prefs.status).toBe('idle');

    // Clear recorded calls
    core.calls.length = 0;

    // A Ready arrives for NIGHT_OWLS
    onEvents([new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })]);
    await settle();

    const updates = core.calls
      .filter((call) => call.method === 'presenceUpdate')
      .map((call) => call.args);
    expect(updates).toContainEqual([NIGHT_OWLS, PresenceStatus.Idle]);
  });

  // Fails if a server whose snapshot avatarHash differs from ownAvatar does not get avatarUpload on Ready.
  it('a server that missed the picture gets it after Ready', async () => {
    const core = freshStore();
    setReadyHook((serverKey) => {
      catchUpPresence(serverKey);
      catchUpAvatar(serverKey);
    });

    // Set own avatar in core
    const newHash = core.avatarSet('/local/avatar.png');
    expect(newHash).toBeDefined();

    core.calls.length = 0;

    // Server NIGHT_OWLS sends Ready with an old avatar hash (or undefined)
    const oldSnapshot = {
      ...snapshot(NIGHT_OWLS),
      selfUser: { ...user(SELF_ID), avatarHash: undefined },
    };
    onEvents([new MobileEvent.Ready({ snapshot: oldSnapshot })]);
    await settle();

    const uploads = core.calls
      .filter((call) => call.method === 'avatarUpload')
      .map((call) => call.args);
    expect(uploads).toEqual([[NIGHT_OWLS]]);

    // If a server already knows the new avatar, no upload is sent
    core.calls.length = 0;
    const currentSnapshot = {
      ...snapshot(NIGHT_OWLS),
      selfUser: { ...user(SELF_ID), avatarHash: newHash },
    };
    onEvents([new MobileEvent.Ready({ snapshot: currentSnapshot })]);
    await settle();

    const laterUploads = core.calls.filter((call) => call.method === 'avatarUpload');
    expect(laterUploads).toHaveLength(0);
  });

  // Fails if a refusal during avatar setting does not show a toast naming the server.
  it('one refusal names its server', async () => {
    const core = freshStore();
    // NIGHT_OWLS will fail avatarUpload
    core.failNext('avatarUpload', new MobileError.Server({ code: 'file_too_large', message: 'too big' }));

    await setAvatar('/local/new_photo.png');

    const toasts = useAppStore.getState().toasts;
    expect(toasts.some((t) => t.text.includes('Night Owls kept your old picture: That file is bigger than this server allows.'))).toBe(true);
  });
});
