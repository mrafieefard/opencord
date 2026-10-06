#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Text,
    /// Shown in Phase 1 but not joinable yet.
    Voice,
    Category,
}

impl ChannelKind {
    /// Stable name, as stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Voice => "voice",
            Self::Category => "category",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "text" => Some(Self::Text),
            "voice" => Some(Self::Voice),
            "category" => Some(Self::Category),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for kind in [ChannelKind::Text, ChannelKind::Voice, ChannelKind::Category] {
            assert_eq!(ChannelKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ChannelKind::parse("forum"), None);
    }
}
