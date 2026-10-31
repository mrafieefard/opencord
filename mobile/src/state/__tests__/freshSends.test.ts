import { forgetFreshSend, isFreshSend, markFreshSend } from '../freshSends';

describe('fresh sends', () => {
  it("a send rises once, within 2 s, and the server's copy doesn't rise again", () => {
    markFreshSend('n1', 1_000);
    expect(isFreshSend('n1', 2_500)).toBe(true);
    // Read twice before it has risen (React's double render): still fresh.
    expect(isFreshSend('n1', 2_600)).toBe(true);
    forgetFreshSend('n1');
    expect(isFreshSend('n1', 2_700)).toBe(false);
  });

  it('an old send, or a message with no nonce, just appears', () => {
    markFreshSend('n2', 1_000);
    expect(isFreshSend('n2', 3_500)).toBe(false);
    expect(isFreshSend(undefined)).toBe(false);
  });
});
