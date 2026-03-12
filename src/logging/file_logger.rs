//! Process-type thread state and logging init (tracing subscriber, file per level).
//!
//! **Process type:** set_process_type / clear_process_type for thread tagging (still in log context); file output is **by level only**.
//! **Files by level:** One file per level: error, warn, info, debug, trace — e.g. `rct-error-{node_id}.log` (all process types write to the same file per level).
//! **init_logging:** Rotation by day and size, no limit on rotated file count; thread id and name in each line; flush every 64KB so cat/tail/grep see data; lossy=false so no log loss.

use crate::logging::config::LoggingConfig;
use crate::logging::daily_rotating_writer::{
    DailyRotatingFileWriter, MAIN_NON_BLOCKING_QUEUE_LINES,
};
use crate::logging::process_type::ProcessType;
use crate::logging::{common::get_or_create_node_id, statistics};
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::sync::Mutex;
use tracing::Level;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::{
    filter::{FilterExt, FilterFn},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

thread_local! {
    static CURRENT_PROCESS_TYPE: RefCell<Option<ProcessType>> = RefCell::new(None);
}

/// Set process type for the current thread.
pub fn set_process_type(process_type: ProcessType) {
    CURRENT_PROCESS_TYPE.with(|cell| {
        *cell.borrow_mut() = Some(process_type);
    });
}

/// Clear process type for the current thread.
pub fn clear_process_type() {
    CURRENT_PROCESS_TYPE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Get process type for the current thread (Option to avoid clone in filter).
#[inline]
fn get_current_process_type() -> Option<ProcessType> {
    CURRENT_PROCESS_TYPE.with(|cell| *cell.borrow())
}

/// Initialize logging: one file per level (error, warn, info, debug, trace); all process types write to the same file per level. Rotation by day and size; thread id and name in output.
///
/// **Must be called exactly once** per process (e.g. from main at startup). Worker threads are kept alive by storing their guards in a static; dropping guards would stop workers and lose queued log lines.
///
/// **Write path:** event → queue (non_blocking, lossy=false) → one worker thread per file → BufWriter → file. One line per event (with \\n); no interleaving. Flush every 64KB so cat/tail/grep see data. No limit on number of rotated files.
/// **Errors:** If the file writer returns an error (e.g. disk full, permission), the worker may stop and subsequent logs for that level can be lost until restart; ensure log_dir is writable and disk has space.
pub fn init_logging(config: LoggingConfig) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&config.log_dir)?;
    statistics::init_statistics(config.max_file_size);

    let node_id = get_or_create_node_id();
    let mut guards = Vec::new();

    // When enable_debug_file (LOG_DEBUG_FILE=1), allow DEBUG so events reach the debug file layer.
    let effective_level = if config.enable_debug_file {
        "debug"
    } else {
        config.log_level.as_str()
    };
    let base_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(effective_level));
    let filter_str = format!(
        "{},odbc_api=error,odbc_sys=error,rskafka=warn",
        effective_level
    );
    let env_filter = EnvFilter::try_new(&filter_str).unwrap_or_else(|_| base_filter.clone());
    let odbc_warning_filter = FilterFn::new(|metadata| {
        if metadata.level() == &Level::WARN {
            let target = metadata.target();
            if target.contains("odbc") || target.contains("ODBC") {
                return false;
            }
        }
        true
    });
    let registry = tracing_subscriber::registry()
        .with(env_filter.clone())
        .with(odbc_warning_filter);

    let rest_filter = env_filter.clone();

    macro_rules! file_layer {
        ($cfg:expr, $guards:expr, $node_id:expr, $rest:expr, $level:expr, $level_str:expr) => {{
            let suffix = format!("{}-{}", $level_str, $node_id);
            let writer = DailyRotatingFileWriter::new(
                $cfg.log_dir.clone(),
                $cfg.log_file_name.clone(),
                suffix,
                $cfg.max_file_size,
            );
            let thread_name = format!("log-{}", $level_str);
            let (non_blocking, guard) =
                tracing_appender::non_blocking::NonBlockingBuilder::default()
                    .buffered_lines_limit(MAIN_NON_BLOCKING_QUEUE_LINES)
                    .lossy(false)
                    .thread_name(thread_name.as_str())
                    .finish(writer);
            $guards.push(guard);
            let level_filter = $level;
            let filter_fn = FilterFn::new(move |metadata| metadata.level() == &level_filter);
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_timer(ChronoLocal::rfc_3339())
                .with_ansi(false)
                .with_target(true)
                .with_line_number(false)
                .with_file(false)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_filter(filter_fn.and($rest.clone()))
        }};
    }

    let registry = registry
        .with(file_layer!(
            config,
            guards,
            node_id,
            rest_filter,
            Level::ERROR,
            "error"
        ))
        .with(file_layer!(
            config,
            guards,
            node_id,
            rest_filter,
            Level::WARN,
            "warn"
        ))
        .with(file_layer!(
            config,
            guards,
            node_id,
            rest_filter,
            Level::INFO,
            "info"
        ))
        .with(file_layer!(
            config,
            guards,
            node_id,
            rest_filter,
            Level::DEBUG,
            "debug"
        ))
        .with(file_layer!(
            config,
            guards,
            node_id,
            rest_filter,
            Level::TRACE,
            "trace"
        ));

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_timer(ChronoLocal::rfc_3339())
        .with_ansi(true)
        .with_target(false)
        .with_line_number(false)
        .with_file(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_filter(rest_filter);

    registry.with(console_layer).init();

    static GUARDS: Lazy<Mutex<Vec<tracing_appender::non_blocking::WorkerGuard>>> =
        Lazy::new(|| Mutex::new(Vec::new()));
    // Keep guards alive for process lifetime so worker threads never exit (and never drop queued logs).
    {
        let mut static_guards = GUARDS.lock().unwrap();
        static_guards.clear();
        static_guards.extend(guards);
    }

    Ok(())
}
