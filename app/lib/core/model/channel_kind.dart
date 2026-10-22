enum ChannelKind {
  text,

  /// Read-only for most members; shown with a megaphone (§4.2).
  announcement,
  voice,
  category;

  bool get isTextLike => this == text || this == announcement;
}
