--[=[
    Opencord Plugin API Type Definitions
    Provides autocomplete and type checking for Opencord server scripts.
]=]

export type EventName =
    | "start"
    | "stop"
    | "update"
    | "settings_change"
    | "permission_change"
    | "message_create"
    | "message_update"
    | "message_delete"
    | "typing_start"
    | "channel_create"
    | "channel_update"
    | "channel_delete"
    | "plugin_channel_create"
    | "plugin_channel_delete"
    | "view_open"
    | "view_close"
    | "file_uploaded"
    | "role_create"
    | "role_update"
    | "role_delete"
    | "member_join"
    | "member_leave"
    | "member_update"
    | "member_unban"
    | "presence_update"
    | "invite_create"
    | "invite_revoke"
    | "server_update"
    | "voice_settings_update"
    | "upload_settings_update"
    | "voice_state_update"
    | "stream_create"
    | "stream_update"
    | "stream_delete"
    | "stream_viewers_update"
    | "soundboard_play"
    | "soundboard_sound_create"
    | "soundboard_sound_update"
    | "soundboard_sound_delete"
    | "music_now_playing"
    | "music_queue_update"
    | "music_notice"

export type User = {
    id: string,
    name: string,
    plugin: string?,
}

export type Attachment = {
    id: string,
    filename: string,
    kind: string,
    size: number,
}

export type Message = {
    id: string,
    channel_id: string,
    author: User,
    content: string,
    created_at_ms: number,
    edited_at_ms: number?,
    interaction: { user: User, command: string }?,
    attachments: { Attachment },
}

export type ChannelOverwrite = {
    target_kind: string,
    target_id: string,
    allow: { string },
    deny: { string },
}

export type Channel = {
    id: string,
    name: string,
    kind: string,
    topic: string?,
    position: number,
    parent_id: string?,
    overwrites: { ChannelOverwrite },
    bitrate: number,
    user_limit: number,
    text_in_voice: boolean,
    plugin_id: string?,
    type_key: string?,
}

export type PermissionName =
    | "view_channel"
    | "send_messages"
    | "read_history"
    | "manage_messages"
    | "manage_channels"
    | "manage_roles"
    | "kick_members"
    | "ban_members"
    | "create_invite"
    | "manage_server"
    | "attach_files"
    | "mention_everyone"
    | "change_nickname"
    | "manage_nicknames"
    | "send_voice_messages"
    | "manage_plugins"
    | "connect"
    | "speak"
    | "video"
    | "screenshare"
    | "mute_members"
    | "deafen_members"
    | "move_members"
    | "priority_speaker"
    | "use_voice_activity"
    | "use_soundboard"
    | "use_external_sounds"
    | "manage_soundboard"
    | "music_add"
    | "music_control"
    | "music_manage_queue"
    | "music_manage"
    | "music_all_local_songs"
    | "administrator"

export type Role = {
    id: string,
    name: string,
    color: number,
    position: number,
    permissions: { PermissionName },
    hoist: boolean,
    mentionable: boolean,
    max_upload_bytes: number?,
    plugin_id: string?,
}

export type Member = {
    id: string,
    name: string,
    nickname: string?,
    roles: { string },
    role_ids: { string },
    avatar_hash: string?,
    joined_at_ms: number,
}

export type Ban = {
    user: { id: string, name: string },
    reason: string?,
}

export type Invite = {
    code: string,
    uses: number,
    max_uses: number?,
    expires_at_ms: number?,
    address: string?,
}

export type ServerInfo = {
    id: string,
    server_id: string,
    name: string,
    description: string,
    owner_id: string?,
    open_join: boolean,
    everyone_role_id: string,
    icon_hash: string,
}

export type VoiceState = {
    channel_id: string,
    user: { id: string, name: string, plugin: string? },
    self_mute: boolean,
    self_deaf: boolean,
    server_mute: boolean,
    server_deaf: boolean,
    video: boolean,
    streaming: boolean,
}

export type MusicTrack = {
    title: string,
    artist: string,
    duration_ms: number,
    is_stream: boolean,
    is_seekable: boolean,
    source: string,
    source_display_name: string,
    uri: string,
    artwork_id: string,
    local_song_id: string,
    removable: boolean,
}

export type MusicQueueItem = {
    item_id: string,
    requester_id: string,
    added_at_ms: number,
    track: MusicTrack?,
}

export type MusicState = {
    channel_id: string,
    version: number,
    state: "idle" | "loading" | "playing" | "paused" | "unspecified",
    now_playing: MusicQueueItem?,
    queue: { MusicQueueItem },
    history: { MusicQueueItem },
    position_ms: number,
    server_time_ms: number,
    repeat: "off" | "queue" | "track" | "unspecified",
    shuffle: boolean,
    waiting_for_node: boolean,
    paused_empty: boolean,
}

export type FileInfo = {
    path: string,
    size: number,
    public: boolean,
    updated_at_ms: number,
}

export type HttpRequestOptions = {
    method: string?,
    url: string,
    headers: { [string]: string }?,
    body: string?,
    timeout: number?,
}

export type HttpResponse = {
    status: number,
    headers: { [string]: string },
    body: string,
}

export type WebHookRequest = {
    method: string,
    path: string,
    query: { [string]: string },
    headers: { [string]: string },
    body: string,
}

export type WebHookResponse = {
    status: number,
    headers: { [string]: string }?,
    body: string?,
}

export type DbTx = {
    exec: (sql: string, ...any) -> { changes: number, last_id: string },
    query: (sql: string, ...any) -> { { [string]: any } },
}

export type ViewPlace = {
    channel_id: string,
}

export type View = {
    id: string,
    user: User,
    place: ViewPlace,
    platform: string,
    send: (self: View, name: string, data: any) -> boolean,
}

export type UploadedFile = {
    id: string,
    name: string,
    size: number,
    content_type: string,
}

export type ChannelTypeSpec = {
    name: string,
    description: string?,
    icon: string?,
}

export type ViewOpenEvent = {
    view: View,
}

export type ViewCloseEvent = {
    view: View,
    reason: string,
}

export type FileUploadedEvent = {
    view: View,
    file: UploadedFile,
}

export type PluginChannelCreateEvent = {
    channel: Channel,
}

export type PluginChannelDeleteEvent = {
    channel_id: string,
}

-- Globals

declare opencord: {
    id: string,
    version: string,
    --- Permission: none
    on: (event: EventName, handler: (payload: any) -> ()) -> (),
    --- Permission: none
    off: (event: EventName, handler: (payload: any) -> ()) -> (),
    --- Permission: none
    has_permission: (permission: string) -> boolean,
}

declare log: {
    --- Permission: none
    info: (...any) -> (),
    --- Permission: none
    warn: (...any) -> (),
    --- Permission: none
    error: (...any) -> (),
}

declare http: {
    --- Permission: internet
    request: (options: HttpRequestOptions) -> HttpResponse,
}

declare db: {
    --- Permission: none
    exec: (sql: string, ...any) -> { changes: number, last_id: string },
    --- Permission: none
    query: (sql: string, ...any) -> { { [string]: any } },
    --- Permission: none
    transaction: (fn: (tx: DbTx) -> any) -> any,
}

declare messages: {
    --- Permission: send_messages, attach_files (when sending files)
    send: (channel_id: string, content: string | { content: string?, files: { string }? }) -> Message,
    --- Permission: send_messages
    edit: (id: string, content: string | { content: string }) -> Message,
    --- Permission: send_messages
    delete: (id: string) -> (),
    --- Permission: read_messages
    fetch: (channel_id: string, options: { before: string?, limit: number? }?) -> { Message },
}

declare typing: {
    --- Permission: send_messages
    start: (channel_id: string) -> (),
}

export type Command = {
    name: string,
    options: { [string]: any },
    user: User,
    channel_id: string,
    --- Answers the member. Within 3 seconds, or 15 minutes after `defer`.
    reply: (self: Command, content: string, options: { only_you: boolean? }?) -> (),
    --- Shows "is thinking…" and gives up to 15 minutes to reply. Within the first 3 seconds.
    defer: (self: Command, options: { only_you: boolean? }?) -> (),
}

export type Autocomplete = {
    name: string,
    option: string,
    value: string,
    user: User,
    channel_id: string,
    --- Offers up to 25 choices, within 1 second.
    suggest: (self: Autocomplete, choices: { { name: string, value: string } }) -> boolean,
}

export type CommandOption = {
    name: string,
    description: string,
    kind: "text" | "integer" | "boolean" | "choice" | "user" | "channel",
    required: boolean?,
    autocomplete: ((ac: Autocomplete) -> ())?,
    choices: { { name: string, value: string } }?,
    min: number?,
    max: number?,
}

declare commands: {
    --- Permission: register_commands
    register: (spec: { name: string, description: string, options: { CommandOption }? }, run: (cmd: Command) -> ()) -> (),
    --- Permission: register_commands
    unregister: (name: string) -> (),
}

declare channels: {
    --- Permission: read_channels
    list: () -> { Channel },
    --- Permission: read_channels
    get: (id: string) -> Channel?,
    --- Permission: manage_channels
    create: (spec: { kind: string, name: string, topic: string?, parent_id: string?, type_key: string? }) -> Channel,
    --- Permission: manage_channels
    update: (id: string, spec: { name: string?, topic: string?, parent_id: string? }) -> Channel,
    --- Permission: manage_channels
    delete: (id: string) -> (),
    --- Permission: manage_channels
    reorder: (order: { string }) -> (),
    --- Permission: manage_channels, manage_roles
    set_overwrite: (channel_id: string, spec: { target: "role" | "member", id: string, allow: { PermissionName }?, deny: { PermissionName }? }) -> (),
}

declare roles: {
    --- Permission: read_roles
    list: () -> { Role },
    --- Permission: manage_roles
    create: (spec: { name: string, color: number?, permissions: { PermissionName }?, hoist: boolean?, mentionable: boolean? }) -> Role,
    --- Permission: manage_roles
    update: (id: string, spec: { name: string?, color: number?, permissions: { PermissionName }?, hoist: boolean?, mentionable: boolean? }) -> Role,
    --- Permission: manage_roles
    delete: (id: string) -> (),
    --- Permission: manage_roles
    reorder: (order: { string }) -> (),
}

declare members: {
    --- Permission: read_members
    list: () -> { Member },
    --- Permission: read_members
    get: (id: string) -> Member?,
    --- Permission: manage_roles
    add_role: (user_id: string, role_id: string) -> (),
    --- Permission: manage_roles
    remove_role: (user_id: string, role_id: string) -> (),
    --- Permission: manage_nicknames
    set_nickname: (user_id: string, nickname: string?) -> (),
    --- Permission: kick_members
    kick: (user_id: string, reason: string?) -> (),
    --- Permission: ban_members
    ban: (user_id: string, reason: string?) -> (),
    --- Permission: ban_members
    unban: (user_id: string) -> (),
}

declare bans: {
    --- Permission: ban_members
    list: () -> { Ban },
}

declare invites: {
    --- Permission: read_invites
    list: () -> { Invite },
    --- Permission: create_invites
    create: (spec: { max_uses: number?, expires_in_s: number? }?) -> Invite,
}

declare server: {
    --- Permission: read_server
    get: () -> ServerInfo,
}

declare voice: {
    --- Permission: read_voice
    states: (channel_id: string?) -> { VoiceState },
    --- Permission: move_members
    move: (user_id: string, channel_id: string) -> (),
    --- Permission: move_members
    disconnect: (user_id: string) -> (),
    --- Permission: mute_members
    mute: (user_id: string, mute: boolean) -> (),
    --- Permission: deafen_members
    deafen: (user_id: string, deafen: boolean) -> (),
}

declare soundboard: {
    --- Permission: play_soundboard
    play: (channel_id: string, sound_id: string) -> (),
}

declare music: {
    --- Permission: read_music
    state: (channel_id: string) -> MusicState,
    --- Permission: control_music
    add: (channel_id: string, query: string) -> (),
    --- Permission: control_music
    skip: (channel_id: string) -> (),
    --- Permission: control_music
    pause: (channel_id: string) -> (),
    --- Permission: control_music
    resume: (channel_id: string) -> (),
}

declare files: {
    --- Permission: none. Server only (when putting an uploaded file).
    put: (path: string, data: string | buffer | UploadedFile, options: { public: boolean? }?) -> (),
    --- Permission: none
    get: (path: string) -> buffer?,
    --- Permission: none
    delete: (path: string) -> (),
    --- Permission: none
    list: (prefix: string?) -> { FileInfo },
    --- Permission: none. A token for the view's member to load the file at
    --- `path`, good for an hour. Server only.
    token: (path: string, view: View | { id: string }) -> string,
}

declare ui: {
    --- Permission: create_channel_types. Server only.
    register_channel_type: (key: string, spec: ChannelTypeSpec) -> (),
    --- Server only.
    on: (name: string, handler: (view: View, data: any) -> ()) -> (),
    --- Server only.
    views: (place: ViewPlace?) -> { View },
    --- Server only.
    send_place: (place: ViewPlace, name: string, data: any) -> number,
    --- Server only.
    send_role: (role_id: string, name: string, data: any) -> number,
    --- Server only.
    send_all: (name: string, data: any) -> number,
}

declare hooks: {
    --- Permission: receive_web_requests
    on: (path: string, handler: (request: WebHookRequest) -> WebHookResponse) -> (),
}

declare timer: {
    --- Permission: none
    after: (seconds: number, handler: () -> ()) -> number,
    --- Permission: none
    every: (seconds: number, handler: () -> ()) -> number,
    --- Permission: none
    cron: (expr: string, handler: () -> ()) -> number,
    --- Permission: none
    cancel: (timer_id: number) -> (),
}

declare plugins: {
    --- Permission: talk_to_plugins
    emit: (name: string, data: any) -> (),
    --- Permission: talk_to_plugins
    on: (plugin_id: string, name: string, handler: (data: any) -> ()) -> (),
}

declare config: {
    --- Permission: none
    get: (key: string) -> any,
}

--------------------------------------------------------------------------------
--- Client only
--------------------------------------------------------------------------------

export type IconName =
    | "add"
    | "arrow_back"
    | "arrow_forward"
    | "arrow_up"
    | "arrow_down"
    | "check"
    | "close"
    | "delete"
    | "edit"
    | "search"
    | "settings"
    | "info"
    | "warning"
    | "error"
    | "star"
    | "person"
    | "group"
    | "chat"
    | "send"
    | "image"
    | "link"
    | "lock"
    | "lock_open"
    | "refresh"
    | "play"
    | "pause"
    | "stop"
    | "music"
    | "mic"
    | "volume"
    | "schedule"
    | "history"
    | "pin"
    | "tag"
    | "copy"
    | "download"
    | "upload"
    | "more"
    | "menu"
    | "visibility"
    | "visibility_off"
    | "palette"
    | "extension"

export type ColorName =
    | "text"
    | "text_secondary"
    | "text_muted"
    | "background"
    | "surface"
    | "elevated"
    | "border"
    | "accent"
    | "on_accent"
    | "selected"
    | "hover"
    | "danger"

export type Color = ColorName | string | { from: ColorName | string, to: ColorName | string, angle: number? }
export type Insets = number | { top: number?, right: number?, bottom: number?, left: number? } | { x: number?, y: number? }
export type Size = number | "fill" | "fit"
export type Stroke = { width: number?, color: Color? }
export type SelectOption = { value: any, label: string, icon: (IconName | string)? }
export type MenuItem = { value: any, label: string, icon: (IconName | string)?, danger: boolean? }
export type CanvasItem = { id: string, [string]: any }
export type CanvasPoint = { x: number, y: number, pressure: number? }

export type CommonProps = {
    key: (string | number)?,
    width: Size?,
    height: Size?,
    min_width: Size?,
    max_width: Size?,
    min_height: Size?,
    max_height: Size?,
    flex: number?,
    padding: Insets?,
    opacity: number?,
    visible: boolean?,
    tooltip: string?,
    label: string?,
    animate: any?,
    enter: any?,
    exit: any?,
    x: number?,
    y: number?,
    right: number?,
    bottom: number?,
    on_animation_end: (() -> ())?,
}

export type ContainerProps = CommonProps & {
    background: Color?,
    border: Stroke?,
    radius: number?,
    shadow: ("none" | "small" | "medium" | "large")?,
}

export type BlockNode = {
    block: string,
    props: any?,
    children: (BlockNode | { BlockNode })?,
}

export type RowProps = ContainerProps & {
    gap: number?,
    align: ("start" | "center" | "end" | "space_between" | "space_around")?,
    cross: ("start" | "center" | "end" | "stretch")?,
    wrap: boolean?,
}

export type ColumnProps = ContainerProps & {
    gap: number?,
    align: ("start" | "center" | "end" | "space_between" | "space_around")?,
    cross: ("start" | "center" | "end" | "stretch")?,
    wrap: boolean?,
}

export type StackProps = ContainerProps & {}

export type GridProps = ContainerProps & {
    columns: number?,
    min_column_width: number?,
    gap: number?,
}

export type ScrollProps = ContainerProps & {
    direction: ("vertical" | "horizontal")?,
    on_scroll: ((position: number) -> ())?,
    on_reach_start: (() -> ())?,
    on_reach_end: (() -> ())?,
}

export type BoxProps = ContainerProps & {
    on_press: (() -> ())?,
    on_double_press: (() -> ())?,
    on_long_press: (() -> ())?,
    on_secondary_press: (() -> ())?,
    on_hover_enter: (() -> ())?,
    on_hover_exit: (() -> ())?,
}

export type SpacerProps = CommonProps & {
    size: number?,
}

export type DividerProps = CommonProps & {
    color: Color?,
    thickness: number?,
}

export type ListProps = ContainerProps & {
    count: number,
    reverse: boolean?,
    gap: number?,
    item_height: number?,
    item: (index: number) -> BlockNode?,
    on_scroll: ((position: number) -> ())?,
    on_reach_start: (() -> ())?,
    on_reach_end: (() -> ())?,
}

export type TextProps = CommonProps & {
    text: string,
    size: number?,
    weight: ("regular" | "medium" | "semibold" | "bold")?,
    italic: boolean?,
    color: Color?,
    align: ("start" | "center" | "end")?,
    max_lines: number?,
    selectable: boolean?,
    font: string?,
}

export type RichTextProps = CommonProps & {
    text: string,
}

export type ImageProps = CommonProps & {
    asset: string?,
    file: string?,
    file_token: string?,
    url: string?,
    attachment: string?,
    bytes: (string | buffer)?,
    fit: ("cover" | "contain" | "fill" | "none")?,
    radius: number?,
    placeholder: Color?,
    version: number?,
    on_load: (() -> ())?,
    on_error: (() -> ())?,
}

export type IconProps = CommonProps & {
    name: IconName | string,
    size: number?,
    color: Color?,
}

export type UserProps = CommonProps & {
    id: string,
    size: number?,
    show_name: boolean?,
}

export type BadgeProps = CommonProps & {
    text: string,
    tone: ("neutral" | "accent" | "danger")?,
}

export type ProgressProps = CommonProps & {
    value: number?,
    style: ("bar" | "ring")?,
}

export type SpinnerProps = CommonProps & {
    size: number?,
}

export type ButtonProps = CommonProps & {
    text: string?,
    variant: ("primary" | "secondary" | "danger" | "plain")?,
    icon: (IconName | string)?,
    loading: boolean?,
    disabled: boolean?,
    on_press: (() -> ())?,
    on_long_press: (() -> ())?,
}

export type IconButtonProps = CommonProps & {
    icon: IconName | string,
    disabled: boolean?,
    on_press: (() -> ())?,
}

export type CheckboxProps = CommonProps & {
    value: boolean?,
    text: string?,
    disabled: boolean?,
    on_change: ((value: boolean) -> ())?,
}

export type SwitchProps = CommonProps & {
    value: boolean?,
    text: string?,
    disabled: boolean?,
    on_change: ((value: boolean) -> ())?,
}

export type RadioProps = CommonProps & {
    value: boolean?,
    text: string?,
    disabled: boolean?,
    on_change: ((value: boolean) -> ())?,
}

export type SliderProps = CommonProps & {
    value: number?,
    min: number?,
    max: number?,
    step: number?,
    disabled: boolean?,
    on_change: ((value: number) -> ())?,
    on_change_end: ((value: number) -> ())?,
}

export type SelectProps = CommonProps & {
    value: any?,
    options: { SelectOption },
    placeholder: string?,
    disabled: boolean?,
    on_change: ((value: any) -> ())?,
}

export type TabsProps = CommonProps & {
    value: any?,
    options: { SelectOption },
    on_change: ((value: any) -> ())?,
}

export type SegmentedProps = CommonProps & {
    value: any?,
    options: { SelectOption },
    on_change: ((value: any) -> ())?,
}

export type TextInputProps = CommonProps & {
    value: string?,
    placeholder: string?,
    password: boolean?,
    max_length: number?,
    lines: number?,
    disabled: boolean?,
    on_change: ((text: string) -> ())?,
    on_submit: ((text: string) -> ())?,
    on_focus: (() -> ())?,
    on_blur: (() -> ())?,
}

export type ImagePickerProps = CommonProps & {
    text: string?,
    on_upload: ((file: { id: string, name: string, size: number }) -> ())?,
    on_upload_error: ((err: string) -> ())?,
}

export type FilePickerProps = CommonProps & {
    text: string?,
    accept: { string }?,
    on_upload: ((file: { id: string, name: string, size: number }) -> ())?,
    on_upload_error: ((err: string) -> ())?,
}

export type ModalProps = CommonProps & {
    open: boolean?,
    size: ("small" | "medium" | "large" | "full")?,
    sheet: boolean?,
    dismissible: boolean?,
    title: string?,
    on_dismiss: (() -> ())?,
}

export type SheetProps = CommonProps & {
    open: boolean?,
    dismissible: boolean?,
    on_dismiss: (() -> ())?,
}

export type MenuProps = CommonProps & {
    items: { MenuItem },
    on_select: ((value: any) -> ())?,
}

export type TooltipProps = CommonProps & {
    text: string,
}

export type ToastProps = CommonProps & {
    text: string,
    open: boolean?,
    duration: number?,
    on_dismiss: (() -> ())?,
}

export type CanvasProps = CommonProps & {
    items: { CanvasItem }?,
    ink: Stroke?,
    on_pointer_down: ((event: any) -> ())?,
    on_pointer_move: ((event: any) -> ())?,
    on_pointer_up: ((event: any) -> ())?,
    on_pointer_cancel: ((event: any) -> ())?,
    on_wheel: ((event: any) -> ())?,
    on_pan: ((event: any) -> ())?,
    on_scale: ((event: any) -> ())?,
    on_stroke: ((points: { CanvasPoint }) -> ())?,
}

export type ModalHandle = {
    name: string,
    close: (self: ModalHandle, result: any?) -> (),
    update: (self: ModalHandle, props: any?) -> (),
}

export type ViewViewer = {
    id: string,
    name: string,
    roles: { string },
}

export type ViewPlace = {
    kind: string,
    channel_id: string,
}

export type ViewSize = {
    width: number,
    height: number,
}

export type ViewSafeArea = {
    top: number,
    right: number,
    bottom: number,
    left: number,
}

export type ViewContext = {
    viewer: ViewViewer,
    place: ViewPlace,
    platform: string,
    size: ViewSize,
    safe_area: ViewSafeArea,
    theme: "dark" | "light",
    breakpoint: "compact" | "medium" | "expanded",
    input: "mouse" | "touch",
    text_scale: number,
}

declare ui: {
    row: (props: RowProps?, children: { BlockNode }?) -> BlockNode,
    column: (props: ColumnProps?, children: { BlockNode }?) -> BlockNode,
    stack: (props: StackProps?, children: { BlockNode }?) -> BlockNode,
    grid: (props: GridProps?, children: { BlockNode }?) -> BlockNode,
    scroll: (props: ScrollProps?, child: { BlockNode }?) -> BlockNode,
    box: (props: BoxProps?, child: { BlockNode }?) -> BlockNode,
    spacer: (props: SpacerProps?) -> BlockNode,
    divider: (props: DividerProps?) -> BlockNode,
    list: (props: ListProps) -> BlockNode,
    text: (props: TextProps) -> BlockNode,
    rich_text: (props: RichTextProps) -> BlockNode,
    image: (props: ImageProps) -> BlockNode,
    icon: (props: IconProps) -> BlockNode,
    user: (props: UserProps) -> BlockNode,
    badge: (props: BadgeProps) -> BlockNode,
    progress: (props: ProgressProps?) -> BlockNode,
    spinner: (props: SpinnerProps?) -> BlockNode,
    button: (props: ButtonProps?) -> BlockNode,
    icon_button: (props: IconButtonProps) -> BlockNode,
    checkbox: (props: CheckboxProps?) -> BlockNode,
    switch: (props: SwitchProps?) -> BlockNode,
    radio: (props: RadioProps?) -> BlockNode,
    slider: (props: SliderProps?) -> BlockNode,
    select: (props: SelectProps) -> BlockNode,
    tabs: (props: TabsProps) -> BlockNode,
    segmented: (props: SegmentedProps) -> BlockNode,
    text_input: (props: TextInputProps?) -> BlockNode,
    image_picker: (props: ImagePickerProps?) -> BlockNode,
    file_picker: (props: FilePickerProps?) -> BlockNode,
    modal: (props: ModalProps?, child: { BlockNode }?) -> BlockNode,
    sheet: (props: SheetProps?, child: { BlockNode }?) -> BlockNode,
    menu: (props: MenuProps, child: { BlockNode }?) -> BlockNode,
    tooltip: (props: TooltipProps, child: { BlockNode }?) -> BlockNode,
    toast: (props: ToastProps) -> BlockNode,
    canvas: (props: CanvasProps?) -> BlockNode,
}

declare app: {
    state: any,
    set: (updater: (state: any) -> ()) -> (),
    send: (name: string, data: any?) -> (),
    on: (name: string, handler: (data: any) -> ()) -> (),
    listen: (name: string, handler: (event: any) -> ()) -> (),
    copy: (text: string) -> (),
    on_back: (handler: () -> ()) -> (),
    open_link: (url: string) -> (),
    open_channel: (channel_id: string) -> (),
    play_sound: (asset: string, options: { volume: number?, on_end: (() -> ())? }?) -> (),
    modal: {
        open: (name: string, props: any?) -> ModalHandle,
        ask: (name: string, props: any?) -> any,
        close: (name_or_result: any?) -> (),
        close_all: () -> (),
        is_open: (name: string) -> boolean,
    },
    modals: { [string]: (state: any, props: any) -> BlockNode },
    confirm: (text: string) -> boolean,
    alert: (text: string) -> (),
    prompt: (text: string, default: string?) -> string?,
    after: (seconds: number, handler: () -> ()) -> number,
    every: (seconds: number, handler: () -> ()) -> number,
    cancel: (timer_id: number) -> (),
    on_frame: (handler: (dt: number) -> ()) -> (),
    off_frame: () -> (),
    storage: {
        get: (key: string) -> any?,
        set: (key: string, value: any?) -> (),
        delete: (key: string) -> (),
    },
    viewer: ViewViewer,
    place: ViewPlace,
    platform: string,
    size: ViewSize,
    safe_area: ViewSafeArea,
    theme: "dark" | "light",
    breakpoint: "compact" | "medium" | "expanded",
    input: "mouse" | "touch",
    text_scale: number,
}

declare function init(ctx: ViewContext): any
declare function render(state: any): BlockNode
declare function migrate(old_state: any): any
