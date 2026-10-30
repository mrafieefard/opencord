// Where the server list's search and the voice bar live. On iOS 26, as in
// Apple's apps, search is the tab bar's own search tab and the voice
// connection floats above the tab bar as a glass pill (CallSheet), so the
// Servers screen has neither.
export const chrome: { serversSearch: boolean; serversVoiceBar: boolean; tabBarSearch: boolean; floatingVoice: boolean } = {
  /** The Servers screen has a search field in its header. */
  serversSearch: false,
  /** The Servers screen draws the voice bar at its foot. */
  serversVoiceBar: false,
  /** The tab bar has a search tab. */
  tabBarSearch: true,
  /**
   * The voice bar is the call's own glass pill, floating above the tab bar
   * or the composer, which grows into the call when pulled (CallSheet).
   */
  floatingVoice: true,
};
