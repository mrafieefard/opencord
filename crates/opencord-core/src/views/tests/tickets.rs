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

    // Find the text inputs by placeholder
    let mut title_id = None;
    let mut body_id = None;
    for op in seen.ops() {
        if let Patch::Create {
            id, block, props, ..
        } = op
            && block == "text_input"
        {
            if props.get("placeholder") == Some(&json!("Title")) {
                title_id = Some(id);
            } else if props.get("placeholder") == Some(&json!("Description")) {
                body_id = Some(id);
            }
        }
    }
    let title_id = title_id.expect("expected title text_input");
    let body_id = body_id.expect("expected body text_input");
    let submit_btn = seen.node("button", "Open ticket");

    // 3. Submitting sends ticket.create with typed values
    event(&view, *title_id, "change", json!("Printer on fire"));
    event(
        &view,
        *body_id,
        "change",
        json!("There is smoke coming out of tray 2"),
    );
    event(&view, submit_btn, "press", json!({}));
    let seen = settle(&view, &recorder);

    let created = seen.sent("ticket.create");
    assert_eq!(created.len(), 1, "expected ticket.create sent once");
    assert_eq!(created[0]["title"], "Printer on fire");
    assert_eq!(created[0]["body"], "There is smoke coming out of tray 2");
    assert_eq!(created[0]["priority"], "normal");
}
