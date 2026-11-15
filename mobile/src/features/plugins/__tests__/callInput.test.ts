import { callInputJson, startCallInput } from '../callInput';
import { CALL_INPUT_EVERY_MS } from '../limits';

describe('the call input', () => {
  // Fails if ids aren't decimal strings sorted as numbers without repeats.
  it('is the wire shape, sorted as numbers, no repeats', () => {
    expect(JSON.parse(callInputJson({ members: ['900', '1001', '90', '900'], speaking: ['1001'] }))).toEqual({
      kind: 'call',
      members: ['90', '900', '1001'],
      speaking: ['1001'],
    });
  });

  // Fails if the first input waits, a change is lost, a burst sends more
  // than one more, or the burst's last lists aren't the ones sent.
  it('sends once at the start, then at most once per interval with the newest lists', () => {
    jest.useFakeTimers();
    const sent: { members: string[]; speaking: string[] }[] = [];
    let lists = { members: ['1', '2'], speaking: [] as string[] };
    const listeners = new Set<() => void>();
    const stop = startCallInput({
      read: () => lists,
      subscribe: (changed) => {
        listeners.add(changed);
        return () => listeners.delete(changed);
      },
      send: (json) => sent.push(JSON.parse(json) as { members: string[]; speaking: string[] }),
    });
    expect(sent).toEqual([{ kind: 'call', members: ['1', '2'], speaking: [] }]);
    lists = { members: ['1', '2', '3'], speaking: [] };
    listeners.forEach((changed) => changed());
    jest.advanceTimersByTime(CALL_INPUT_EVERY_MS);
    expect(sent).toHaveLength(2);
    for (let n = 0; n < 10; n++) {
      lists = { members: ['1', '2', '3'], speaking: n % 2 === 0 ? ['2'] : ['3'] };
      listeners.forEach((changed) => changed());
      jest.advanceTimersByTime(5);
    }
    jest.advanceTimersByTime(CALL_INPUT_EVERY_MS);
    expect(sent).toHaveLength(3);
    expect(sent[2]?.speaking).toEqual(['3']);
    // Nothing changed: nothing sent.
    listeners.forEach((changed) => changed());
    jest.advanceTimersByTime(CALL_INPUT_EVERY_MS * 3);
    expect(sent).toHaveLength(3);
    stop();
    expect(listeners.size).toBe(0);
    jest.useRealTimers();
  });
});
