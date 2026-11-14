local function init_db()
    db.exec([[
        CREATE TABLE IF NOT EXISTS stats (
            key TEXT PRIMARY KEY,
            count INTEGER NOT NULL DEFAULT 0
        )
    ]])
    db.exec([[
        INSERT OR IGNORE INTO stats (key, count) VALUES ('greetings', 0)
    ]])
end

local function get_banned_words()
    local raw = config.get("banned_words")
    if type(raw) ~= "string" or raw == "" then
        return {}
    end
    local list = {}
    for word in string.gmatch(raw, "[^,%s]+") do
        table.insert(list, string.lower(word))
    end
    return list
end

local function roll(cmd)
    local sides = 6
    if cmd.options and cmd.options.sides then
        local parsed = tonumber(cmd.options.sides)
        if parsed then
            sides = math.floor(parsed)
        end
    end
    if sides < 2 then
        sides = 2
    elseif sides > 1000 then
        sides = 1000
    end

    local rolled = math.random(1, sides)
    cmd:reply("You rolled " .. tostring(rolled) .. " (1-" .. tostring(sides) .. ")")
end

opencord.on("start", function()
    init_db()

    commands.register({
        name = "roll",
        description = "Rolls a die with the given number of sides (2-1000).",
        options = {
            {
                name = "sides",
                description = "Number of sides on the die (2-1000, default 6)",
                kind = "integer",
                required = false,
            },
        },
    }, roll)

    log.info("welcome bot started")
end)

opencord.on("message_create", function(event)
    local msg = event.message
    if not msg or not msg.content then
        return
    end

    local banned = get_banned_words()
    if #banned == 0 then
        return
    end

    local content = string.lower(msg.content)
    for _, word in ipairs(banned) do
        if string.find(content, word, 1, true) then
            messages.delete(msg.id)
            log.warn("deleted message containing banned word: " .. word)
            return
        end
    end
end)

opencord.on("member_join", function(event)
    local channel_id = config.get("welcome_channel")
    if not channel_id or channel_id == "" then
        return
    end

    local member = event.member
    local name = member and member.name or "new member"
    local id = member and member.id or ""

    messages.send(channel_id, "Welcome <@" .. id .. "> (" .. name .. ") to the server!")
    db.exec("UPDATE stats SET count = count + 1 WHERE key = 'greetings'")
    log.info("greeted new member " .. name)
end)
