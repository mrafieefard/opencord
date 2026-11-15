-- Paint's screen: one canvas filling the view, the tools floating over it.
--
-- Everyone draws on the same board of 1600 × 1000 units. It's drawn as
-- large as it fits, centred, so everyone sees the same picture at any tile
-- size; points from the pointer are turned back into board units before
-- they're sent.

local BOARD_WIDTH = 1600
local BOARD_HEIGHT = 1000
-- The server's limits: points per stroke, strokes per board.
local MAX_POINTS = 100
local MAX_STROKES = 300
-- Ink strokes being drawn at once (fingers); more are left undrawn.
local MAX_INKING = 10
-- A stroke's numbers from the renderer: x, y, pressure.
local PER_POINT = 3

local COLORS = {
    { value = "text", label = "Ink" },
    { value = "#e03131", label = "Red" },
    { value = "#f59f00", label = "Yellow" },
    { value = "#2f9e44", label = "Green" },
    { value = "#1c7ed6", label = "Blue" },
}
local WIDTHS = {
    { value = 2, label = "Thin" },
    { value = 6, label = "Medium" },
    { value = 14, label = "Thick" },
}

-- Ink stroke id → its numbers so far. An entry leaves when its stroke's
-- last batch (`done`) comes.
local inking = {}
local inking_count = 0

-- The board's scale and where its top left corner is in the view.
local function fit()
    local size = app.size
    local scale = math.min(size.width / BOARD_WIDTH, size.height / BOARD_HEIGHT)
    local left = (size.width - BOARD_WIDTH * scale) / 2
    local top = (size.height - BOARD_HEIGHT * scale) / 2
    return scale, left, top
end

local function others_line(members)
    local others = 0
    for _, id in ipairs(members) do
        if id ~= app.viewer.id then
            others += 1
        end
    end
    if others == 0 then
        return "Just you"
    elseif others == 1 then
        return "You and 1 other"
    end
    return "You and " .. others .. " others"
end

-- Adds a stroke the board doesn't have yet, dropping the oldest past the
-- server's limit as it does.
local function add(state, stroke)
    if state.ids[stroke.id] then
        return
    end
    table.insert(state.strokes, stroke)
    state.ids[stroke.id] = true
    if #state.strokes > MAX_STROKES then
        local oldest = table.remove(state.strokes, 1)
        state.ids[oldest.id] = nil
    end
end

-- At most MAX_POINTS of a stroke's points, evenly spread, keeping its first
-- and last; in board units, whole, on the board.
local function to_board(numbers)
    local scale, left, top = fit()
    local count = #numbers // PER_POINT
    if scale <= 0 or count == 0 then
        return nil
    end
    local kept = math.min(count, MAX_POINTS)
    local points = table.create(kept * 2)
    for i = 0, kept - 1 do
        local at = if kept == 1 then 0 else math.round(i * (count - 1) / (kept - 1))
        local x = (numbers[at * PER_POINT + 1] - left) / scale
        local y = (numbers[at * PER_POINT + 2] - top) / scale
        table.insert(points, math.clamp(math.round(x), 0, BOARD_WIDTH))
        table.insert(points, math.clamp(math.round(y), 0, BOARD_HEIGHT))
    end
    return points
end

local function finish(id, numbers)
    local points = to_board(numbers)
    if points == nil then
        return
    end
    local stroke = { id = id, color = app.state.color, width = app.state.width, points = points }
    app.set(function(s)
        add(s, stroke)
    end)
    app.send("stroke", stroke)
end

-- The canvas's `stroke` event: `{ stroke, points, done }`, points flat
-- x, y, pressure in the view's pixels.
local function on_stroke(event)
    local numbers = inking[event.stroke]
    if numbers == nil then
        if inking_count >= MAX_INKING then
            return
        end
        numbers = {}
        inking[event.stroke] = numbers
        inking_count += 1
    end
    for _, n in ipairs(event.points) do
        table.insert(numbers, n)
    end
    if event.done then
        inking[event.stroke] = nil
        inking_count -= 1
        finish(event.stroke, numbers)
    end
end

function init(ctx)
    app.send("board")
    return {
        strokes = {},
        ids = {},
        color = COLORS[1].value,
        width = WIDTHS[2].value,
        line = others_line(app.call.members),
    }
end

-- A stroke anyone in the call drew, this viewer's own too.
app.on("stroke", function(stroke)
    app.set(function(s)
        add(s, stroke)
    end)
end)

-- The board so far, in parts, when the view opens.
app.on("strokes", function(data)
    app.set(function(s)
        for _, stroke in ipairs(data.strokes) do
            add(s, stroke)
        end
    end)
end)

app.on("clear", function()
    app.set(function(s)
        s.strokes = {}
        s.ids = {}
    end)
end)

app.listen("call_members_change", function(change)
    app.set(function(s)
        s.line = others_line(change.members)
    end)
end)

local function items(s)
    local scale, left, top = fit()
    local list = {
        {
            id = "board",
            type = "rect",
            x = left,
            y = top,
            w = BOARD_WIDTH * scale,
            h = BOARD_HEIGHT * scale,
            fill = "surface",
        },
    }
    for _, stroke in ipairs(s.strokes) do
        local points = table.create(#stroke.points)
        for i = 1, #stroke.points, 2 do
            points[i] = left + stroke.points[i] * scale
            points[i + 1] = top + stroke.points[i + 1] * scale
        end
        table.insert(list, {
            id = stroke.id,
            type = "path",
            points = points,
            stroke = { color = stroke.color, width = stroke.width * scale },
        })
    end
    return list, scale
end

local function swatch(s, color)
    local chosen = s.color == color.value
    return ui.box({
        width = 24,
        height = 24,
        radius = 12,
        background = color.value,
        border = { width = if chosen then 3 else 1, color = if chosen then "accent" else "border" },
        tooltip = color.label,
        label = color.label,
        on_press = function()
            app.set(function(state)
                state.color = color.value
            end)
        end,
    })
end

local function tools(s)
    local children = {}
    for _, color in ipairs(COLORS) do
        table.insert(children, swatch(s, color))
    end
    table.insert(
        children,
        ui.segmented({
            options = WIDTHS,
            value = s.width,
            on_change = function(change)
                app.set(function(state)
                    state.width = change.value
                end)
            end,
        })
    )
    table.insert(
        children,
        ui.button({
            text = "Clear",
            variant = "secondary",
            on_press = function()
                app.send("clear")
            end,
        })
    )
    table.insert(children, ui.text({ text = s.line, color = "text_secondary" }))
    return ui.box({
        x = 8,
        y = 8,
        right = 8,
        padding = 8,
        radius = 10,
        background = "elevated",
        shadow = "small",
    }, {
        ui.row({ gap = 8, cross = "center", wrap = true }, children),
    })
end

function render(s)
    local list, scale = items(s)
    return ui.stack({ width = "fill", height = "fill" }, {
        ui.canvas({
            items = list,
            ink = { color = s.color, width = s.width * scale },
            on_stroke = on_stroke,
        }),
        tools(s),
    })
end
