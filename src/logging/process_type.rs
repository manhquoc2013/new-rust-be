//! Process type for log classification (VETC/VDTC/BETC/Local), guard, macro with_process_type.

/// Process type for log classification (VETC/VDTC/BETC/Local).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessType {
    VETC,
    VDTC,
    BETC,
    Local,
}

impl ProcessType {
    /// From boo_type: 1=VETC, 2=VDTC, else BETC.
    pub fn from_boo_type(boo_type: i32) -> Self {
        match boo_type {
            1 => ProcessType::VETC,
            2 => ProcessType::VDTC,
            _ => ProcessType::BETC,
        }
    }

    /// Process type name (lowercase).
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessType::VETC => "vetc",
            ProcessType::VDTC => "vdtc",
            ProcessType::BETC => "betc",
            ProcessType::Local => "local",
        }
    }

    /// Process type name (uppercase).
    pub fn as_str_upper(&self) -> &'static str {
        match self {
            ProcessType::VETC => "VETC",
            ProcessType::VDTC => "VDTC",
            ProcessType::BETC => "BETC",
            ProcessType::Local => "LOCAL",
        }
    }

    /// All process types except Local.
    pub fn all_data_process_types() -> &'static [ProcessType] {
        &[ProcessType::VETC, ProcessType::VDTC, ProcessType::BETC]
    }

    /// All process types (including Local).
    pub fn all() -> &'static [ProcessType] {
        &[
            ProcessType::VETC,
            ProcessType::VDTC,
            ProcessType::BETC,
            ProcessType::Local,
        ]
    }
}

/// Macro to create tracing span with process_type.
#[macro_export]
macro_rules! with_process_type {
    ($process_type:expr, $block:block) => {{
        let span = tracing::span!(
            tracing::Level::TRACE,
            "process",
            process_type = $process_type.as_str()
        );
        let _guard = span.enter();
        $block
    }};
}

/// Guard clears process_type on drop (including on panic).
pub struct ProcessTypeGuard;

impl ProcessTypeGuard {
    /// Create guard and set process_type cho thread hiện tại.
    pub fn new(process_type: ProcessType) -> Self {
        crate::logging::file_logger::set_process_type(process_type);
        ProcessTypeGuard
    }
}

impl Drop for ProcessTypeGuard {
    fn drop(&mut self) {
        crate::logging::file_logger::clear_process_type();
    }
}

/// Run async block in a tracing span with process_type.
pub async fn with_process_type_async<F, Fut, T>(process_type: ProcessType, fut: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let span = tracing::span!(
        tracing::Level::TRACE,
        "process",
        process_type = process_type.as_str()
    );
    let _guard = span.enter();
    fut().await
}
