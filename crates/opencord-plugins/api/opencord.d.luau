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
    | "command"
    | "command_autocomplete"
    | "channel_create"
    | "channel_update"
    | "channel_delete"
    | "plugin_channel_create"
    | "plugin_channel_delete"
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
    display_name: string?,
    avatar_url: string?,
    bot: boolean,
}

export type Attachment = {
    id: string,
    filename: string,
    size_bytes: number,
    content_type: string,
    url: string,
}

export type Message = {
    id: string,
    channel_id: string,
    author: User,
    content: string,
    created_at_ms: number,
    updated_at_ms: number?,
    attachments: { Attachment },
}

export type Channel = {
    id: string,
    name: string,
    kind: string,
    topic: string?,
    position: number,
    parent_id: string?,
}

export type Role = {
    id: string,
    name: string,
    color: number,
    position: number,
    permissions: number,
    hoist: boolean,
    mentionable: boolean,
}

export type Member = {
    user: User,
    nickname: string?,
    roles: { string },
    joined_at_ms: number,
}

export type Ban = {
    user: User,
    reason: string?,
}

export type Invite = {
    code: string,
    channel_id: string,
    inviter: User?,
    uses: number,
    max_uses: number,
    created_at_ms: number,
    expires_at_ms: number?,
}

export type ServerInfo = {
    id: string,
    name: string,
    description: string?,
    icon_url: string?,
    owner_id: string,
}

export type VoiceState = {
    channel_id: string,
    user_id: string,
    session_id: string,
    mute: boolean,
    deafen: boolean,
    self_mute: boolean,
    self_deafen: boolean,
}

export type MusicTrack = {
    title: string,
    artist: string,
    duration_ms: number,
    uri: string?,
    source: string,
}

export type MusicState = {
    channel_id: string,
    playing: boolean,
    current_track: MusicTrack?,
    queue: { MusicTrack },
    position_ms: number,
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
    exec: (sql: string, ...any) -> { rows_affected: number, last_insert_rowid: number },
    query: (sql: string, ...any) -> { { [string]: any } },
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
    exec: (sql: string, ...any) -> { rows_affected: number, last_insert_rowid: number },
    --- Permission: none
    query: (sql: string, ...any) -> { { [string]: any } },
    --- Permission: none
    transaction: (fn: (tx: DbTx) -> any) -> any,
}

declare messages: {
    --- Permission: send_messages
    send: (channel_id: string, content: string | { content: string, files: { string }? }) -> Message,
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

declare commands: {
    --- Permission: register_commands
    register: (spec: { name: string, description: string, options: { any }? }) -> (),
    --- Permission: register_commands
    unregister: (name: string) -> (),
}

declare channels: {
    --- Permission: read_channels
    list: () -> { Channel },
    --- Permission: read_channels
    get: (id: string) -> Channel?,
    --- Permission: manage_channels
    create: (spec: { kind: string, name: string, topic: string?, parent_id: string? }) -> Channel,
    --- Permission: manage_channels
    update: (id: string, spec: { name: string?, topic: string?, parent_id: string? }) -> Channel,
    --- Permission: manage_channels
    delete: (id: string) -> (),
    --- Permission: manage_channels
    reorder: (order: { string }) -> (),
    --- Permission: manage_channels
    set_overwrite: (channel_id: string, target: { role_id: string? } | { user_id: string? }, allow: number, deny: number) -> (),
}

declare roles: {
    --- Permission: read_roles
    list: () -> { Role },
    --- Permission: manage_roles
    create: (spec: { name: string, color: number?, permissions: number?, hoist: boolean?, mentionable: boolean? }) -> Role,
    --- Permission: manage_roles
    update: (id: string, spec: { name: string?, color: number?, permissions: number?, hoist: boolean?, mentionable: boolean? }) -> Role,
    --- Permission: manage_roles
    delete: (id: string) -> (),
    --- Permission: manage_roles
    reorder: (order: { string }) -> (),
}

declare members: {
    --- Permission: read_members
    list: (options: { limit: number?, after: string? }?) -> { Member },
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
    create: (spec: { channel_id: string, max_uses: number?, expires_in_seconds: number? }?) -> Invite,
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
    add: (channel_id: string, query: string) -> { MusicTrack },
    --- Permission: control_music
    skip: (channel_id: string) -> (),
    --- Permission: control_music
    pause: (channel_id: string) -> (),
    --- Permission: control_music
    resume: (channel_id: string) -> (),
}

declare files: {
    --- Permission: none
    put: (path: string, data: string | buffer, options: { public: boolean? }?) -> FileInfo,
    --- Permission: none
    get: (path: string) -> buffer?,
    --- Permission: none
    delete: (path: string) -> (),
    --- Permission: none
    list: (prefix: string?) -> { FileInfo },
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
