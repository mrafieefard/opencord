// SPDX-License-Identifier: MPL-2.0

//! Ticket system example client view test (plan M2 Task 26).

use opencord_plugins::view::Patch;
use serde_json::json;

use crate::views::test_support::{event, open, settle, spec};

// Fails if the initial render doesn't show "No tickets yet." or doesn't request
// the ticket list, if pressing "New ticket" doesn't open the named modal,
// or if submitting doesn't send "ticket.create" with the typed title, body, and priority.
#[test]
fn ticket_client_script_opens_modal_and_submits_ticket() {
    let client_script =
        match std::fs::read_to_string("../../examples/plugins/tickets/client/main.luau") {
            Ok(s) => s,
            Err(_) => std::fs::read_to_string("examples/plugins/tickets/client/main.luau")
                .expect("tickets client/main.luau must exist"),
        };
    let mut spec = spec(&client_script);
    spec.show_errors = true;
    let (view, recorder) = open(spec);
    let seen = recorder.first_batch();
    if !seen.errors.is_empty() {
        panic!("client errors: {:?}", seen.errors);
    }
    for op in seen.ops() {
        if let Patch::Error { message, details } = op {
            panic!("render error op: {message}: {details:?}");
        }
    }

    // 1. Initial render contains "No tickets yet."
    let _ = seen.node("text", "No tickets yet.");
    assert!(
        !seen.sent("ticket.list").is_empty(),
        "must send ticket.list on init"
    );

    // 2. Pressing "New ticket" opens the modal root
    let new_ticket_btn = seen.node("button", "New ticket");
    event(&view, new_ticket_btn, "press", json!({}));
    let seen = settle(&view, &recorder);

    let (_main, modals) = seen.roots();
    assert_eq!(modals.len(), 1, "expected 1 modal open");
    assert_eq!(modals[0].name, "new_ticket");

    // Find the text inputs by placeholder and segmented control
    let mut title_id = None;
    let mut body_id = None;
    let mut priority_id = None;
    for op in seen.ops() {
        if let Patch::Create {
            id, block, props, ..
        } = op
        {
            if block == "text_input" {
                if props.get("placeholder") == Some(&json!("Title")) {
                    title_id = Some(id);
                } else if props.get("placeholder") == Some(&json!("Description")) {
                    body_id = Some(id);
                }
            } else if block == "segmented" {
                priority_id = Some(id);
            }
        }
    }
    let title_id = title_id.expect("expected title text_input");
    let body_id = body_id.expect("expected body text_input");
    let priority_id = priority_id.expect("expected priority segmented");
    let submit_btn = seen.node("button", "Open ticket");

    // 3. Submitting sends ticket.create with typed values
    // Fails if on_change handler does not read .value from data table.
    event(
        &view,
        *title_id,
        "change",
        json!({"value": "Printer on fire"}),
    );
    event(
        &view,
        *body_id,
        "change",
        json!({"value": "There is smoke coming out of tray 2"}),
    );
    event(&view, *priority_id, "change", json!({"value": "high"}));
    event(&view, submit_btn, "press", json!({}));
    let seen = settle(&view, &recorder);

    for op in seen.ops() {
        if let Patch::Error { message, details } = op {
            panic!("unexpected Patch::Error after change: {message} ({details:?})");
        }
    }

    let created = seen.sent("ticket.create");
    assert_eq!(created.len(), 1, "expected ticket.create sent once");
    assert_eq!(created[0]["title"], "Printer on fire");
    assert_eq!(created[0]["body"], "There is smoke coming out of tray 2");
    assert_eq!(created[0]["priority"], "high");

    // 4. Receiving a ticket.list with one ticket renders badges without error
    // Fails if badge uses "variant" instead of "tone".
    view.server_message(
        "ticket.list".to_string(),
        json!({
            "tickets": [
                {
                    "id": 1,
                    "title": "Printer on fire",
                    "body": "There is smoke coming out of tray 2",
                    "state": "open",
                    "priority": "high",
                    "author_id": "123",
                    "author_name": "Alice"
                }
            ]
        })
        .to_string(),
    );
    let seen = settle(&view, &recorder);
    for op in seen.ops() {
        if let Patch::Error { message, details } = op {
            panic!("unexpected Patch::Error on ticket.list: {message} ({details:?})");
        }
    }
    let _ = seen.node("text", "Printer on fire");

    // 5. Press the ticket's row to open ticket detail
    // Change that must fail it: changing tone to variant on priority badge at client/main.luau:234 causes Patch::Error on ticket detail render
    let mut ticket_row = None;
    for op in seen.ops() {
        if let Patch::Create {
            id, block, events, ..
        } = op
            && block == "box"
            && events.iter().any(|e| e == "press")
        {
            ticket_row = Some(*id);
            break;
        }
    }
    let ticket_row = ticket_row.expect("expected ticket row with press event");

    event(&view, ticket_row, "press", json!({}));
    let seen = settle(&view, &recorder);
    for op in seen.ops() {
        if let Patch::Error { message, details } = op {
            panic!("unexpected Patch::Error opening ticket detail: {message} ({details:?})");
        }
    }
    assert!(
        !seen.sent("ticket.open").is_empty(),
        "must send ticket.open on pressing ticket row"
    );

    // Also verify receiving ticket.detail with ticket and replies
    view.server_message(
        "ticket.detail".to_string(),
        json!({
            "ticket": {
                "id": 1,
                "title": "Printer on fire",
                "body": "There is smoke coming out of tray 2",
                "state": "open",
                "priority": "high",
                "author_id": "123",
                "author_name": "Alice"
            },
            "replies": []
        })
        .to_string(),
    );
    let seen = settle(&view, &recorder);
    for op in seen.ops() {
        if let Patch::Error { message, details } = op {
            panic!("unexpected Patch::Error on ticket.detail: {message} ({details:?})");
        }
    }
}
