//! Fixed limits on plugins. Limits an admin can change (package size,
//! memory, handler time, database and file space) live in the server's
//! `[plugins]` config instead.

use std::time::Duration;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

/// Unpacked size of all of a package's files together.
pub const PACKAGE_MAX_UNPACKED_BYTES: u64 = 200 * MIB;
pub const PACKAGE_MAX_FILES: usize = 2_000;
/// Unpacked size of one file in a package.
pub const PACKAGE_MAX_FILE_BYTES: u64 = 20 * MIB;
/// Unpacked over packed size, checked for entries larger than
/// [`PACKAGE_RATIO_MIN_BYTES`] unpacked.
pub const PACKAGE_MAX_RATIO: u64 = 100;
pub const PACKAGE_RATIO_MIN_BYTES: u64 = MIB;
/// A path inside a package, in bytes.
pub const PACKAGE_PATH_MAX_BYTES: usize = 200;

/// Running `manifest.lua`.
pub const MANIFEST_TIME: Duration = Duration::from_millis(100);
pub const MANIFEST_MEMORY_BYTES: usize = 4 * MIB as usize;

/// Loading all server scripts and running `start` gets this many times the
/// handler time.
pub const START_TIME_FACTOR: u32 = 10;

/// A plugin may use [`CPU_SHARE_MAX_PERMILLE`] of a core, averaged over this.
pub const CPU_SHARE_WINDOW: Duration = Duration::from_secs(10);
/// Past this its events wait in its queue: thousandths of one core.
pub const CPU_SHARE_MAX_PERMILLE: u32 = 250;

/// Events waiting for one plugin; past this, the oldest are dropped.
pub const EVENT_QUEUE: usize = 10_000;

/// This many failed handlers within [`FAILURE_WINDOW`] restart the plugin.
pub const FAILURES_BEFORE_RESTART: usize = 5;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(10 * 60);
/// This many restarts within [`RESTART_WINDOW`] turn the plugin off.
pub const RESTARTS_BEFORE_OFF: usize = 5;
pub const RESTART_WINDOW: Duration = Duration::from_secs(60 * 60);
/// Restarts wait 1, 2, 4 … seconds, at most [`RESTART_BACKOFF_MAX`].
pub const RESTART_BACKOFF_FIRST: Duration = Duration::from_secs(1);
pub const RESTART_BACKOFF_MAX: Duration = Duration::from_secs(60);
/// How long a stopping plugin may finish what it is doing.
pub const STOP_GRACE: Duration = Duration::from_secs(1);

/// Rows one query of the plugin's database returns.
pub const DB_MAX_ROWS: usize = 10_000;
/// One SQL statement on the plugin's database.
pub const DB_STATEMENT_TIME: Duration = Duration::from_secs(5);

/// Internet requests one plugin has open at once.
pub const HTTP_CONCURRENT: usize = 10;
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
pub const HTTP_MAX_RESPONSE_BYTES: u64 = 10 * MIB;
pub const HTTP_MAX_REQUEST_BYTES: u64 = 10 * MIB;
pub const HTTP_MAX_REDIRECTS: usize = 5;

pub const WEBHOOK_MAX_BODY_BYTES: u64 = MIB;
/// Web hook calls one plugin takes per [`WEBHOOK_RATE_PERIOD`].
pub const WEBHOOK_RATE: u32 = 60;
pub const WEBHOOK_RATE_PERIOD: Duration = Duration::from_secs(60);
/// How long a web hook's handler has to answer.
pub const WEBHOOK_TIME: Duration = Duration::from_secs(10);

/// How long a `/command` handler has to reply.
pub const COMMAND_REPLY_TIME: Duration = Duration::from_secs(3);
/// How long a deferred `/command` has to reply.
pub const COMMAND_THINKING_TIME: Duration = Duration::from_secs(15 * 60);
/// Commands one plugin has thinking at once.
pub const COMMAND_THINKING_MAX: usize = 100;
/// How long an autocomplete handler has to answer.
pub const AUTOCOMPLETE_TIME: Duration = Duration::from_secs(1);

/// Timers one plugin has at once.
pub const MAX_TIMERS: usize = 1_000;
/// The shortest interval of a repeating timer.
pub const TIMER_MIN_EVERY: Duration = Duration::from_secs(1);

/// One message on the plugin bus, as JSON.
pub const BUS_MAX_PAYLOAD_BYTES: usize = 64 * KIB as usize;

/// Log lines kept in memory per plugin.
pub const LOG_LINES: usize = 5_000;
/// `log.txt` is rotated at this size; one old file is kept.
pub const LOG_FILE_MAX_BYTES: u64 = 5 * MIB;
pub const LOG_OLD_FILES: usize = 1;
pub const LOG_LINE_MAX_CHARS: usize = 2_000;

/// How long an upload waits for an admin's review.
pub const PENDING_REVIEW_TIME: Duration = Duration::from_secs(60 * 60);

/// A dev token's lifetime, in days.
pub const DEV_TOKEN_MIN_DAYS: u32 = 1;
pub const DEV_TOKEN_MAX_DAYS: u32 = 30;
pub const DEV_TOKEN_DEFAULT_DAYS: u32 = 7;
/// Dev pushes one token makes per second.
pub const DEV_PUSHES_PER_SECOND: u32 = 2;

// Views: a plugin's client script, running on a member's device (§11, §12).

/// One view's client VM.
pub const VIEW_MEMORY_BYTES: usize = 32 * MIB as usize;
/// One client handler (a press, a server event, `render`).
pub const VIEW_HANDLER_TIME: Duration = Duration::from_millis(50);
/// One `app.on_frame` call.
pub const VIEW_FRAME_TIME: Duration = Duration::from_millis(8);
/// Loading every `require`d client file and running `init` gets this many
/// times the handler time.
pub const VIEW_START_TIME_FACTOR: u32 = 10;

/// Blocks in one tree a view draws.
pub const VIEW_MAX_BLOCKS: usize = 20_000;
/// How deeply blocks nest in one tree; the root is 1 deep.
pub const VIEW_MAX_DEPTH: usize = 64;
/// One string prop, in characters.
pub const VIEW_MAX_PROP_CHARS: usize = 10_000;
/// The `text` prop of `text` and `rich_text`, in characters.
pub const VIEW_MAX_TEXT_CHARS: usize = 20_000;
/// Everything one tree's props carry, counted as each string's bytes plus
/// 8 for every other value. Many blocks can share one long Lua string or
/// table, so the copies the tree makes need a cap of their own.
pub const VIEW_MAX_TREE_BYTES: usize = 16 * MIB as usize;
/// Options in one `menu`, `select`, `tabs` or `segmented`.
pub const VIEW_MAX_OPTIONS: usize = 100;
/// Drawing items in one canvas.
pub const CANVAS_MAX_ITEMS: usize = 20_000;
/// Points in one canvas path.
pub const CANVAS_MAX_PATH_POINTS: usize = 10_000;
/// Items of a lazy list drawn at once.
pub const LIST_MAX_RANGE: usize = 200;
/// Named modals one view has open at once.
pub const VIEW_MAX_MODALS: usize = 20;
/// Handlers of one view waiting at once on a quick dialog or
/// `app.modal.ask`.
pub const VIEW_MAX_WAITING: usize = 100;

/// Views one session has open.
pub const VIEWS_PER_SESSION: usize = 4;
/// Events the server sends to one view, a second.
pub const TO_VIEW_EVENTS_PER_SECOND: u32 = 60;
pub const TO_VIEW_BYTES_PER_SECOND: u64 = 512 * KIB;
/// Events one view sends to the server, a second.
pub const FROM_VIEW_EVENTS_PER_SECOND: u32 = 30;
pub const FROM_VIEW_BYTES_PER_SECOND: u64 = 128 * KIB;
/// One view event's data, as JSON.
pub const VIEW_EVENT_MAX_BYTES: usize = 64 * KIB as usize;

/// An `image { bytes }`, before base64.
pub const IMAGE_MAX_RAW_BYTES: usize = 256 * KIB as usize;
/// An image the server fetches for `image { url }`.
pub const IMAGE_FETCH_MAX_BYTES: u64 = 10 * MIB;
pub const IMAGE_FETCH_MAX_PIXELS: u64 = 40_000_000;
/// Fetched images the server keeps per plugin, and for how long.
pub const IMAGE_CACHE_MAX_BYTES: u64 = 100 * MIB;
pub const IMAGE_CACHE_FRESH: Duration = Duration::from_secs(60 * 60);
/// Fetched images the server keeps per plugin, however small.
pub const IMAGE_CACHE_MAX_ENTRIES: usize = 10_000;
/// Fetches of images not in the cache one member may cause, per period.
pub const IMAGE_FETCHES_PER_MEMBER: u32 = 20;
pub const IMAGE_FETCH_PERIOD: Duration = Duration::from_secs(10);
/// A `files.token` works this long.
pub const FILE_TOKEN_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// Live file tokens one plugin holds: each is a different file or member,
/// since asking again for the same pair renews its token.
pub const FILE_TOKENS_PER_PLUGIN: usize = 10_000;

/// `app.storage`, per plugin per server on one device.
pub const DEVICE_STORAGE_MAX_BYTES: usize = MIB as usize;
pub const DEVICE_STORAGE_KEY_MAX_CHARS: usize = 200;

/// Functions one view gives `app.on` and `app.listen`, together.
pub const VIEW_MAX_LISTENERS: usize = 1_000;
/// Timers one view has at once.
pub const VIEW_MAX_TIMERS: usize = 100;
/// The shortest interval of a view's repeating timer.
pub const VIEW_TIMER_MIN_EVERY: Duration = Duration::from_millis(100);
/// The longest delay of a view's timer, as on the server.
pub const VIEW_TIMER_MAX: Duration = Duration::from_secs(365 * 24 * 60 * 60);
/// Sounds of one view waiting to call their `on_end`.
pub const VIEW_MAX_SOUNDS: usize = 100;
/// Side effects (links, sounds, copies) waiting for one view's next batch:
/// they're Rust's memory, not under the VM's cap.
pub const VIEW_MAX_EFFECTS: usize = 1_000;
/// An `app.open_link` address.
pub const VIEW_LINK_MAX_CHARS: usize = 2_000;

/// Channel types one plugin registers.
pub const CHANNEL_TYPES_PER_PLUGIN: usize = 20;
/// Activities one plugin registers.
pub const ACTIVITIES_PER_PLUGIN: usize = 20;
/// The text of `app.confirm`, `app.alert` and `app.prompt`.
pub const DIALOG_MAX_CHARS: usize = 2_000;
/// One view sends its plugin's log at most one error report this often.
pub const VIEW_ERROR_REPORT_EVERY: Duration = Duration::from_secs(10);
pub const VIEW_ERROR_REPORT_MAX_CHARS: usize = 2_000;
