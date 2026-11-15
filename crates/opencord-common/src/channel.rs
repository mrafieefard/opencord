#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Text,
    /// Shown in Phase 1 but not joinable yet.
    Voice,
    Category,
    /// Drawn by a plugin's view; holds no messages or voice (design
    /// 2026-11-07 §14.2).
    Plugin,
}

impl ChannelKind {
    /// Stable name, as stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Voice => "voice",
            Self::Category => "category",
            Self::Plugin => "plugin",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "text" => Some(Self::Text),
            "voice" => Some(Self::Voice),
            "category" => Some(Self::Category),
            "plugin" => Some(Self::Plugin),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for kind in [
            ChannelKind::Text,
            ChannelKind::Voice,
            ChannelKind::Category,
            ChannelKind::Plugin,
        ] {
            assert_eq!(ChannelKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ChannelKind::parse("forum"), None);
    }
}
