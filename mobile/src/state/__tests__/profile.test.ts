import { MobileEvent } from '@opencord/core-native';

import { NIGHT_OWLS, SELF_ID, snapshot, user } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import { catchUpName } from '../actions/profile';
import { onEvents, setReadyHook } from '../store';

describe('the display name', () => {
  it('a Ready under another name sends the saved one', async () => {
    const core = freshStore();
    setReadyHook(catchUpName);
    onEvents([new MobileEvent.Ready({ snapshot: { ...snapshot(NIGHT_OWLS), selfUser: { ...user(SELF_ID), displayName: 'Old name' } } })]);
    await settle();
    expect(core.calls.filter((call) => call.method === 'profileUpdate').map((call) => call.args)).toEqual([[NIGHT_OWLS, 'Ava']]);
    // Under the same name, nothing is sent.
    onEvents([new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })]);
    await settle();
    expect(core.calls.filter((call) => call.method === 'profileUpdate')).toHaveLength(1);
  });
});
