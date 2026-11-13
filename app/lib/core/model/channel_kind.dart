enum ChannelKind {
  text,

  /// Read-only for most members; shown with a megaphone (§4.2).
  announcement,
  voice,
  category,

  /// Plugin custom channels (plugins design §14).
  plugin;

  bool get isTextLike => this == text || this == announcement;
}
