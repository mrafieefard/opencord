// Where the server list's search and the voice bar live. On iOS 26, as in
// Apple's apps, search is the tab bar's own search tab and the voice
// connection rides above the tab bar (VoiceAccessory), so the Servers
// screen has neither.
export const chrome: { serversSearch: boolean; serversVoiceBar: boolean; tabBarSearch: boolean } = {
  /** The Servers screen has a search field in its header. */
  serversSearch: false,
  /** The Servers screen draws the voice bar at its foot. */
  serversVoiceBar: false,
  /** The tab bar has a search tab and the voice accessory. */
  tabBarSearch: true,
};
