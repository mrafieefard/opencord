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
