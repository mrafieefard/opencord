function init(ctx)
    app.send("ticket.list")
    return {
        tickets = {},
        is_support = false,
        selected_id = nil,
        selected_ticket = nil,
        replies = {},
        new_title = "",
        new_body = "",
        new_priority = "normal",
        reply_body = "",
        error = nil,
    }
end

app.on("ticket.list", function(data)
    app.set(function(s)
        s.tickets = data.tickets or {}
        if data.is_support ~= nil then
            s.is_support = data.is_support
        end
    end)
end)

app.on("ticket.created", function(data)
    app.set(function(s)
        table.insert(s.tickets, 1, data.ticket)
    end)
end)

app.on("ticket.detail", function(data)
    app.set(function(s)
        s.selected_ticket = data.ticket
        s.selected_id = data.ticket.id
        s.replies = data.replies or {}
    end)
end)

app.on("ticket.replied", function(data)
    app.set(function(s)
        if s.selected_id == data.ticket_id then
            table.insert(s.replies, data.reply)
        end
    end)
end)

app.on("ticket.closed", function(data)
    app.set(function(s)
        for _, t in ipairs(s.tickets) do
            if t.id == data.ticket.id then
                t.state = "closed"
            end
        end
        if s.selected_id == data.ticket.id and s.selected_ticket then
            s.selected_ticket.state = "closed"
        end
    end)
end)

app.on("ticket.error", function(data)
    app.set(function(s)
        s.error = data.message
    end)
end)

app.modals.new_ticket = function(s, props)
    return ui.modal({
        size = "medium",
    }, {
        ui.column({ gap = 12, padding = 16 }, {
            ui.text({ text = "Open New Ticket", weight = "semibold", size = 18 }),
            ui.text_input({
                placeholder = "Title",
                value = s.new_title,
                on_change = function(t)
                    app.set(function(state)
                        state.new_title = (type(t) == "table" and t.value) or t
                    end)
                end,
            }),
            ui.text_input({
                placeholder = "Description",
                lines = 4,
                value = s.new_body,
                on_change = function(b)
                    app.set(function(state)
                        state.new_body = (type(b) == "table" and b.value) or b
                    end)
                end,
            }),
            ui.segmented({
                options = {
                    { label = "Low", value = "low" },
                    { label = "Normal", value = "normal" },
                    { label = "High", value = "high" },
                },
                value = s.new_priority,
                on_change = function(p)
                    app.set(function(state)
                        state.new_priority = (type(p) == "table" and p.value) or p
                    end)
                end,
            }),
            ui.row({ gap = 8, align = "end" }, {
                ui.button({
                    text = "Cancel",
                    variant = "secondary",
                    on_press = function()
                        app.modal.close()
                    end,
                }),
                ui.button({
                    text = "Open ticket",
                    variant = "primary",
                    on_press = function()
                        if s.new_title ~= "" and s.new_body ~= "" then
                            app.send("ticket.create", {
                                title = s.new_title,
                                body = s.new_body,
                                priority = s.new_priority,
                            })
                            app.set(function(state)
                                state.new_title = ""
                                state.new_body = ""
                                state.new_priority = "normal"
                            end)
                            app.modal.close()
                        end
                    end,
                }),
            }),
        }),
    })
end

local function render_ticket_list(s)
    local items = {}
    if #s.tickets == 0 then
        table.insert(
            items,
            ui.column({ align = "center", cross = "center", padding = 32 }, {
                ui.text({ text = "No tickets yet." }),
            })
        )
    else
        for _, t in ipairs(s.tickets) do
            local ticket_id = t.id
            table.insert(
                items,
                ui.box({
                    padding = 12,
                    radius = 8,
                    on_press = function()
                        app.set(function(state)
                            state.selected_id = ticket_id
                            state.selected_ticket = t
                        end)
                        app.send("ticket.open", { id = ticket_id })
                    end,
                }, {
                    ui.column({ gap = 4 }, {
                        ui.row({ align = "space_between", cross = "center" }, {
                            ui.text({ text = t.title, weight = "semibold" }),
                            ui.badge({
                                text = t.state or "open",
                                tone = (t.state == "closed") and "neutral" or "accent",
                            }),
                        }),
                        ui.row({ gap = 8 }, {
                            ui.text({ text = "By " .. (t.author_name or "Unknown"), size = 12 }),
                            ui.text({ text = "• " .. (t.priority or "normal"), size = 12 }),
                        }),
                    }),
                })
            )
        end
    end

    return ui.scroll({ flex = 1 }, {
        ui.column({ gap = 8, padding = 12 }, items),
    })
end

local function render_ticket_detail(s, is_compact)
    if not s.selected_ticket then
        return ui.column({ flex = 1, align = "center", cross = "center", padding = 32 }, {
            ui.text({ text = "Select a ticket to view details." }),
        })
    end

    local t = s.selected_ticket
    local header_children = {}
    if is_compact then
        table.insert(
            header_children,
            ui.button({
                text = "Back",
                variant = "secondary",
                on_press = function()
                    app.set(function(state)
                        state.selected_id = nil
                        state.selected_ticket = nil
                    end)
                end,
            })
        )
    end

    local action_btn
    if s.is_support and t.state ~= "closed" then
        action_btn = ui.button({
            text = "Close ticket",
            variant = "danger",
            on_press = function()
                app.send("ticket.close", { id = t.id })
            end,
        })
    else
        action_btn = ui.spacer({})
    end

    table.insert(
        header_children,
        ui.row({ align = "space_between", cross = "center", flex = 1 }, {
            ui.column({ gap = 4 }, {
                ui.text({ text = t.title, weight = "bold", size = 18 }),
                ui.row({ gap = 8 }, {
                    ui.text({ text = "By " .. (t.author_name or "Unknown"), size = 12 }),
                    ui.badge({
                        text = t.state or "open",
                        tone = (t.state == "closed") and "neutral" or "accent",
                    }),
                    ui.badge({ text = t.priority or "normal", tone = "neutral" }),
                }),
            }),
            action_btn,
        })
    )

    local reply_items = {}
    for _, r in ipairs(s.replies) do
        table.insert(
            reply_items,
            ui.box({ padding = 8, radius = 6 }, {
                ui.column({ gap = 2 }, {
                    ui.text({ text = r.author_name or "Unknown", weight = "semibold", size = 13 }),
                    ui.text({ text = r.body }),
                }),
            })
        )
    end

    return ui.column({ flex = 1, gap = 12, padding = 16 }, {
        ui.column({ gap = 8 }, header_children),
        ui.divider({}),
        ui.text({ text = t.body }),
        ui.divider({}),
        ui.text({ text = "Replies (" .. tostring(#s.replies) .. ")", weight = "semibold" }),
        ui.scroll({ flex = 1 }, {
            ui.column({ gap = 8 }, reply_items),
        }),
        ui.row({ gap = 8 }, {
            ui.text_input({
                flex = 1,
                placeholder = "Write a reply...",
                value = s.reply_body,
                on_change = function(b)
                    app.set(function(state)
                        state.reply_body = (type(b) == "table" and b.value) or b
                    end)
                end,
            }),
            ui.button({
                text = "Reply",
                variant = "primary",
                on_press = function()
                    if s.reply_body ~= "" then
                        app.send("ticket.reply", { id = t.id, body = s.reply_body })
                        app.set(function(state)
                            state.reply_body = ""
                        end)
                    end
                end,
            }),
        }),
    })
end

function render(s)
    local is_compact = app.breakpoint == "compact"
    local top_bar = ui.row({
        align = "space_between",
        cross = "center",
        padding = 12,
    }, {
        ui.text({ text = "Support Tickets", weight = "bold", size = 20 }),
        ui.button({
            text = "New ticket",
            variant = "primary",
            on_press = function()
                app.modal.open("new_ticket")
            end,
        }),
    })

    if is_compact then
        local main_content
        if s.selected_ticket then
            main_content = render_ticket_detail(s, true)
        else
            main_content = render_ticket_list(s)
        end
        return ui.column({ flex = 1 }, {
            top_bar,
            ui.divider({}),
            main_content,
        })
    else
        return ui.column({ flex = 1 }, {
            top_bar,
            ui.divider({}),
            ui.row({ flex = 1 }, {
                ui.column({ width = 320 }, {
                    render_ticket_list(s),
                }),
                ui.divider({}),
                render_ticket_detail(s, false),
            }),
        })
    end
end
