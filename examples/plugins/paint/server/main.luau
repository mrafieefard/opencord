-- Paint: a shared whiteboard for everyone in a call.
--
-- This is a sample. Each activity session has one board, kept in this
-- script's memory: it's gone when the session ends, and when the plugin
-- restarts or the server does. A plugin that must keep what people make
-- stores it with `db` instead (see the tickets example).

-- The board's size in board units, whatever the size of anyone's tile.
local BOARD_WIDTH = 1600
local BOARD_HEIGHT = 1000
-- The most strokes a board keeps; the oldest go first.
local MAX_STROKES = 300
-- A stroke's points: flat x, y pairs, 1 to 100 of them.
local MIN_NUMBERS = 2
local MAX_NUMBERS = 200
local MAX_ID_CHARS = 64
local MIN_WIDTH = 1
local MAX_WIDTH = 24
-- A board goes to someone who opens it later in events of this many strokes.
local STROKES_PER_EVENT = 30

-- The client's palette: the theme's text colour and four others.
local PALETTE = {
    text = true,
    ["#e03131"] = true,
    ["#f59f00"] = true,
    ["#2f9e44"] = true,
    ["#1c7ed6"] = true,
}

-- Session id → { strokes = { oldest first }, ids = { [stroke id] = true } }.
-- An entry leaves when its session ends.
local boards = {}

local function board_of(session_id)
    local board = boards[session_id]
    if board == nil then
        board = { strokes = {}, ids = {} }
        boards[session_id] = board
    end
    return board
end

-- The session a view is open in, or nil for any other view.
local function session_of(view)
    local place = view.place
    if place.kind ~= "activity" or place.activity ~= "paint" then
        return nil
    end
    return place.session_id
end

-- Whether n is a number from low to high (NaN is neither).
local function within(n, low, high)
    return type(n) == "number" and n >= low and n <= high
end

-- The stroke to keep, or nil and why it's dropped.
local function checked(data)
    if type(data) ~= "table" then
        return nil, "it isn't a table"
    end
    local id = data.id
    if type(id) ~= "string" or #id < 1 or utf8.len(id) == nil or utf8.len(id) > MAX_ID_CHARS then
        return nil, "its id isn't 1 to 64 characters"
    end
    if type(data.color) ~= "string" or not PALETTE[data.color] then
        return nil, "its colour isn't in the palette"
    end
    if not within(data.width, MIN_WIDTH, MAX_WIDTH) then
        return nil, "its width isn't from 1 to 24"
    end
    local points = data.points
    if type(points) ~= "table" then
        return nil, "it needs 2 to 200 numbers"
    end
    local count = #points
    if count < MIN_NUMBERS or count > MAX_NUMBERS or count % 2 ~= 0 then
        return nil, "it needs 2 to 200 numbers"
    end
    local kept = table.create(count)
    for i = 1, count, 2 do
        local x, y = points[i], points[i + 1]
        if not within(x, 0, BOARD_WIDTH) or not within(y, 0, BOARD_HEIGHT) then
            return nil, "a point is off the board"
        end
        kept[i], kept[i + 1] = x, y
    end
    return { id = id, color = data.color, width = data.width, points = kept }
end

opencord.on("start", function()
    ui.register_activity("paint", {
        name = "Paint together",
        description = "A shared whiteboard",
        icon = "assets/paint.png",
    })
    log.info("paint started")
end)

-- A view asks for the board when it opens.
ui.on("board", function(view)
    local session_id = session_of(view)
    if session_id == nil then
        return
    end
    local strokes = board_of(session_id).strokes
    for first = 1, #strokes, STROKES_PER_EVENT do
        local last = math.min(first + STROKES_PER_EVENT - 1, #strokes)
        view:send("strokes", { strokes = { table.unpack(strokes, first, last) } })
    end
end)

ui.on("stroke", function(view, data)
    local session_id = session_of(view)
    if session_id == nil then
        return
    end
    local stroke, why = checked(data)
    if stroke == nil then
        -- The reason only: what someone drew is theirs.
        log.warn("dropped a stroke: " .. why)
        return
    end
    local board = board_of(session_id)
    if board.ids[stroke.id] then
        return
    end
    table.insert(board.strokes, stroke)
    board.ids[stroke.id] = true
    if #board.strokes > MAX_STROKES then
        local oldest = table.remove(board.strokes, 1)
        board.ids[oldest.id] = nil
    end
    -- Every view of the session, the sender's too: it skips an id it has.
    ui.send_place({ session_id = session_id }, "stroke", stroke)
end)

-- Anyone in the call may clear the board: only they can open its view.
ui.on("clear", function(view)
    local session_id = session_of(view)
    if session_id == nil then
        return
    end
    boards[session_id] = nil
    ui.send_place({ session_id = session_id }, "clear", {})
end)

opencord.on("activity_end", function(event)
    local session_id = event.session.id
    local board = boards[session_id]
    boards[session_id] = nil
    if board ~= nil then
        log.info(string.format("the board of session %s is gone (%d strokes)", session_id, #board.strokes))
    end
end)
