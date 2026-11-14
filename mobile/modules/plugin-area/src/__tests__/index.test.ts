// Without the native module (jest, or a build without Flutter) the phone
// must still run: nothing throws, and createArea says why it can't.
import * as PluginArea from '../index';

describe('@opencord/plugin-area without its native module', () => {
  it('is not available', () => {
    expect(PluginArea.available).toBe(false);
  });

  // Fails if any call reaches a native module that isn't there.
  it('every call returns without throwing', async () => {
    expect(() => {
      PluginArea.warmUp();
      PluginArea.coolDown();
      PluginArea.destroyArea('area-1');
      PluginArea.destroyAllAreas();
      PluginArea.setSetup('area-1', '{"theme":"dark"}');
      PluginArea.pushBatch('area-1', '{"view":"1","seq":1,"ops":[]}');
      PluginArea.hostReply('area-1', 1, '{"ok":null}');
      PluginArea.addInputListener(() => {}).remove();
      PluginArea.addStatusListener(() => {}).remove();
      PluginArea.addHostCallListener(() => {}).remove();
    }).not.toThrow();
    await expect(PluginArea.back('area-1')).resolves.toBe(false);
  });

  // Fails if createArea resolves (a caller would wait for an area that
  // never draws) or rejects with another reason.
  it('createArea rejects with unavailable', async () => {
    await expect(PluginArea.createArea('area-1', '{"theme":"dark"}')).rejects.toThrow('unavailable');
  });
});
