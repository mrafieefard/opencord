//! What goes inside the encryption (design 2026-10-22 §10.6): a
//! `PrivatePayload`, which servers never decode.

use opencord_common::private::{KEY_LEN, MESSAGE_ID_LEN, SIGNATURE_LEN};
use opencord_common::validation;
use opencord_proto::v1 as proto;
use prost::Message;
use proto::private_payload::Body;

use super::PrivateError;

/// A moved note's server id: the server's id from `Hello`.
const SERVER_ID_LEN: usize = 16;
/// A moved note's server name, in characters.
const SERVER_NAME_MAX_CHARS: usize = 100;

/// A payload that decoded and passed every check.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub message_id: [u8; MESSAGE_ID_LEN],
    /// The author's clock, for information only.
    pub sent_at_ms: i64,
    /// The conversation's other person.
    pub recipient: [u8; KEY_LEN],
    /// The author's permission slip for the recipient.
    pub slip: Option<[u8; SIGNATURE_LEN]>,
    /// Text in an edit or a text is the trimmed text.
    pub body: Body,
}

pub fn build(
    message_id: [u8; MESSAGE_ID_LEN],
    recipient: &[u8; KEY_LEN],
    slip: Option<[u8; SIGNATURE_LEN]>,
    body: Body,
    now_ms: i64,
) -> Vec<u8> {
    proto::PrivatePayload {
        message_id: message_id.to_vec(),
        sent_at_ms: now_ms,
        recipient_public_key: recipient.to_vec(),
        slip: slip.map(|slip| slip.to_vec()).unwrap_or_default(),
        body: Some(body),
    }
    .encode_to_vec()
}

pub fn parse(bytes: &[u8]) -> Result<Parsed, PrivateError> {
    let payload = proto::PrivatePayload::decode(bytes)
        .map_err(|_| invalid("a private payload doesn't decode"))?;
    let message_id = array(&payload.message_id, "a private payload's message id")?;
    let recipient = array(
        &payload.recipient_public_key,
        "a private payload's recipient",
    )?;
    let slip = match payload.slip.len() {
        0 => None,
        _ => Some(array(&payload.slip, "a private payload's slip")?),
    };
    let body = match payload.body {
        None => return Err(invalid("a private payload has no body")),
        Some(Body::Text(text)) => Body::Text(proto::PrivateText {
            content: content(&text.content)?,
        }),
        Some(Body::Edit(edit)) => {
            array::<MESSAGE_ID_LEN>(&edit.message_id, "an edit's message id")?;
            Body::Edit(proto::PrivateEdit {
                message_id: edit.message_id,
                content: content(&edit.content)?,
            })
        }
        Some(Body::Delete(delete)) => {
            array::<MESSAGE_ID_LEN>(&delete.message_id, "a delete's message id")?;
            Body::Delete(delete)
        }
        Some(Body::RelayMoved(moved)) => {
            array::<SERVER_ID_LEN>(&moved.server_id, "a moved note's server id")?;
            let chars = moved.server_name.chars().count();
            if chars == 0 || chars > SERVER_NAME_MAX_CHARS {
                return Err(invalid("a moved note's server name is 1 to 100 characters"));
            }
            Body::RelayMoved(moved)
        }
        Some(body @ (Body::RequestAccepted(_) | Body::SessionReset(_))) => body,
    };
    Ok(Parsed {
        message_id,
        sent_at_ms: payload.sent_at_ms,
        recipient,
        slip,
        body,
    })
}

fn content(text: &str) -> Result<String, PrivateError> {
    validation::message_content(text).map_err(|error| PrivateError::Invalid(error.to_string()))
}

fn array<const N: usize>(bytes: &[u8], what: &str) -> Result<[u8; N], PrivateError> {
    bytes
        .try_into()
        .map_err(|_| invalid(&format!("{what} must be {N} bytes")))
}

fn invalid(message: &str) -> PrivateError {
    PrivateError::Invalid(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(content: &str) -> Body {
        Body::Text(proto::PrivateText {
            content: content.to_owned(),
        })
    }

    fn encoded(change: impl FnOnce(&mut proto::PrivatePayload)) -> Vec<u8> {
        let mut payload = proto::PrivatePayload::decode(
            build([1; 16], &[2; 32], Some([3; 64]), text("hi"), 5).as_slice(),
        )
        .unwrap();
        change(&mut payload);
        payload.encode_to_vec()
    }

    #[test]
    fn a_built_payload_parses_back() {
        let bytes = build([1; 16], &[2; 32], Some([3; 64]), text("  hi  "), 5);
        assert_eq!(
            parse(&bytes).unwrap(),
            Parsed {
                message_id: [1; 16],
                sent_at_ms: 5,
                recipient: [2; 32],
                slip: Some([3; 64]),
                body: text("hi"),
            }
        );
        let without_slip = build(
            [1; 16],
            &[2; 32],
            None,
            Body::SessionReset(Default::default()),
            5,
        );
        assert_eq!(parse(&without_slip).unwrap().slip, None);
    }

    #[test]
    fn wrong_lengths_and_a_missing_body_are_refused() {
        let cases: [fn(&mut proto::PrivatePayload); 6] = [
            |p| p.message_id.truncate(15),
            |p| p.recipient_public_key.push(0),
            |p| p.slip.truncate(63),
            |p| p.body = None,
            |p| p.body = Some(text("   ")),
            |p| p.body = Some(text(&"x".repeat(4_001))),
        ];
        for change in cases {
            assert!(parse(&encoded(change)).is_err());
        }
        assert!(parse(b"\xff\xff\xff").is_err());
    }

    #[test]
    fn edits_deletes_and_moves_are_checked() {
        let edit = |id: Vec<u8>, content: &str| {
            Body::Edit(proto::PrivateEdit {
                message_id: id,
                content: content.to_owned(),
            })
        };
        let moved = |id: Vec<u8>, name: &str| {
            Body::RelayMoved(proto::RelayMoved {
                server_id: id,
                server_name: name.to_owned(),
            })
        };
        let delete = |id: Vec<u8>| Body::Delete(proto::PrivateDelete { message_id: id });
        let accepted = [
            edit(vec![4; 16], "new"),
            delete(vec![4; 16]),
            moved(vec![4; 16], "Home"),
            moved(vec![4; 16], &"é".repeat(100)),
            Body::RequestAccepted(Default::default()),
        ];
        for body in accepted {
            assert!(parse(&encoded(|p| p.body = Some(body))).is_ok());
        }
        let refused = [
            edit(vec![4; 15], "new"),
            edit(vec![4; 16], ""),
            delete(vec![4; 17]),
            moved(vec![4; 15], "Home"),
            moved(vec![4; 16], ""),
            moved(vec![4; 16], &"é".repeat(101)),
        ];
        for body in refused {
            assert!(parse(&encoded(|p| p.body = Some(body))).is_err());
        }
    }
}
