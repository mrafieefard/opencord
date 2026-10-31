import { inviteRoute } from '../inviteLink';

describe('inviteRoute', () => {
  it("invite links open Add server, other links don't", () => {
    expect(inviteRoute('opencord://host:7710/invite/ABC#fp=test')).toBe(
      `/add-server?link=${encodeURIComponent('opencord://host:7710/invite/ABC#fp=test')}`,
    );
    expect(inviteRoute('opencords://example.com/invite/X')).toBe(
      `/add-server?link=${encodeURIComponent('opencords://example.com/invite/X')}`,
    );
    expect(inviteRoute('https://example.com')).toBeNull();
    expect(inviteRoute('opencord-app:///servers')).toBeNull();
    expect(inviteRoute('')).toBeNull();
  });
});
