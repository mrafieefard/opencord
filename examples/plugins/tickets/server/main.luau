local function init_db()
    db.exec([[
        CREATE TABLE IF NOT EXISTS tickets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            channel_id TEXT NOT NULL,
            author_id TEXT NOT NULL,
            author_name TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            priority TEXT NOT NULL DEFAULT 'normal',
            state TEXT NOT NULL DEFAULT 'open',
            created_at INTEGER NOT NULL
        )
    ]])
    db.exec([[
        CREATE TABLE IF NOT EXISTS ticket_replies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ticket_id INTEGER NOT NULL,
            author_id TEXT NOT NULL,
            author_name TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )
    ]])
end

local function has_role(user, role_id)
    if not user or not user.roles or not role_id or role_id == "" then
        return false
    end
    for _, r in ipairs(user.roles) do
        if r == role_id then
            return true
        end
    end
    return false
end

opencord.on("start", function()
    init_db()

    ui.register_channel_type("tickets", {
        name = "Tickets",
        description = "Open and track support tickets",
        icon = "assets/ticket.png",
    })

    log.info("tickets plugin started")
end)

local function broadcast_ticket_event(place, event_name, payload, skip_view_id, author_id)
    local support_role = config.get("support_role")
    for _, v in ipairs(ui.views(place)) do
        if v.id ~= skip_view_id then
            local is_support = false
            if type(support_role) == "string" and support_role ~= "" and v.user and (v.user.roles or v.user.role_ids) then
                local roles = v.user.roles or v.user.role_ids
                for _, r in ipairs(roles) do
                    if r == support_role then
                        is_support = true
                        break
                    end
                end
            end
            if is_support or (author_id and v.user and v.user.id == author_id) then
                v:send(event_name, payload)
            end
        end
    end
end

ui.on("ticket.list", function(view, _data)
    local is_support = has_role(view.user, config.get("support_role"))
    local rows
    if is_support then
        rows = db.query(
            "SELECT id, channel_id, author_id, author_name, title, body, priority, state, created_at FROM tickets WHERE channel_id = ? ORDER BY id DESC",
            view.place.channel_id
        )
    else
        rows = db.query(
            "SELECT id, channel_id, author_id, author_name, title, body, priority, state, created_at FROM tickets WHERE channel_id = ? AND author_id = ? ORDER BY id DESC",
            view.place.channel_id,
            view.user.id
        )
    end
    view:send("ticket.list", {
        tickets = rows or {},
        is_support = is_support,
    })
end)

ui.on("ticket.create", function(view, data)
    if type(data) ~= "table" then
        return
    end

    local title = data.title
    local body = data.body
    if type(title) ~= "string" or title == "" or type(body) ~= "string" or body == "" then
        return
    end

    local priority = data.priority
    if priority ~= "low" and priority ~= "high" then
        priority = "normal"
    end

    local now = os.time()
    local res = db.exec(
        "INSERT INTO tickets (channel_id, author_id, author_name, title, body, priority, state, created_at) VALUES (?, ?, ?, ?, ?, ?, 'open', ?)",
        view.place.channel_id,
        view.user.id,
        view.user.name or "Unknown",
        title,
        body,
        priority,
        now
    )

    local id = tonumber(res.last_id) or res.last_id
    local ticket = {
        id = id,
        channel_id = view.place.channel_id,
        author_id = view.user.id,
        author_name = view.user.name or "Unknown",
        title = title,
        body = body,
        priority = priority,
        state = "open",
        created_at = now,
    }

    local log_channel = config.get("log_channel")
    if type(log_channel) == "string" and log_channel ~= "" then
        local msg = "Ticket #" .. tostring(id) .. " opened by " .. (view.user.name or "Unknown") .. ": " .. title
        local ok, err = pcall(function()
            messages.send(log_channel, msg)
        end)
        if ok then
            log.info("posted to log channel: " .. msg)
        else
            log.warn("failed to post to log channel: " .. tostring(err))
        end
    end

    view:send("ticket.created", { ticket = ticket })
    broadcast_ticket_event(view.place, "ticket.created", { ticket = ticket }, view.id, ticket.author_id)
end)

ui.on("ticket.open", function(view, data)
    if type(data) ~= "table" or not data.id then
        return
    end
    local ticket_id = tonumber(data.id)
    if not ticket_id then
        return
    end

    local rows = db.query(
        "SELECT id, channel_id, author_id, author_name, title, body, priority, state, created_at FROM tickets WHERE id = ? AND channel_id = ?",
        ticket_id,
        view.place.channel_id
    )
    if not rows or #rows == 0 then
        return
    end

    local ticket = rows[1]
    local is_support = has_role(view.user, config.get("support_role"))
    if not is_support and ticket.author_id ~= view.user.id then
        return
    end

    local replies = db.query(
        "SELECT id, ticket_id, author_id, author_name, body, created_at FROM ticket_replies WHERE ticket_id = ? ORDER BY id ASC",
        ticket_id
    )

    view:send("ticket.detail", {
        ticket = ticket,
        replies = replies or {},
    })
end)

ui.on("ticket.reply", function(view, data)
    if type(data) ~= "table" or not data.id or type(data.body) ~= "string" or data.body == "" then
        return
    end
    local ticket_id = tonumber(data.id)
    if not ticket_id then
        return
    end

    local rows = db.query(
        "SELECT id, channel_id, author_id, author_name, title, body, priority, state, created_at FROM tickets WHERE id = ? AND channel_id = ?",
        ticket_id,
        view.place.channel_id
    )
    if not rows or #rows == 0 then
        return
    end

    local ticket = rows[1]
    local is_support = has_role(view.user, config.get("support_role"))
    if not is_support and ticket.author_id ~= view.user.id then
        return
    end

    local now = os.time()
    local res = db.exec(
        "INSERT INTO ticket_replies (ticket_id, author_id, author_name, body, created_at) VALUES (?, ?, ?, ?, ?)",
        ticket_id,
        view.user.id,
        view.user.name or "Unknown",
        data.body,
        now
    )

    local reply = {
        id = tonumber(res.last_id) or res.last_id,
        ticket_id = ticket_id,
        author_id = view.user.id,
        author_name = view.user.name or "Unknown",
        body = data.body,
        created_at = now,
    }

    view:send("ticket.replied", { ticket_id = ticket_id, reply = reply })
    broadcast_ticket_event(view.place, "ticket.replied", { ticket_id = ticket_id, reply = reply }, view.id, ticket.author_id)
end)

ui.on("ticket.close", function(view, data)
    if type(data) ~= "table" or not data.id then
        return
    end
    local ticket_id = tonumber(data.id)
    if not ticket_id then
        return
    end

    local is_support = has_role(view.user, config.get("support_role"))
    if not is_support then
        view:send("ticket.error", { message = "Only support members can close tickets" })
        return
    end

    local rows = db.query(
        "SELECT id, channel_id, author_id, author_name, title, body, priority, state, created_at FROM tickets WHERE id = ? AND channel_id = ?",
        ticket_id,
        view.place.channel_id
    )
    if not rows or #rows == 0 then
        return
    end

    local ticket = rows[1]
    db.exec("UPDATE tickets SET state = 'closed' WHERE id = ?", ticket_id)
    ticket.state = "closed"

    view:send("ticket.closed", { ticket = ticket })
    broadcast_ticket_event(view.place, "ticket.closed", { ticket = ticket }, view.id, ticket.author_id)
end)
