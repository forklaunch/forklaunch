/// [INFO] message in Cyan (with newline)
macro_rules! log_info {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Cyan)))?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// [OK] message in Green (with newline)
macro_rules! log_ok {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Green)))?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// [WARN] message in Yellow (with newline)
macro_rules! log_warn {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Yellow)))?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// [ERROR] message in Red (with newline)
macro_rules! log_error {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Red)))?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// Bold header in specified color (with newline)
macro_rules! log_header {
    ($out:expr, $color:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some($color)).set_bold(true))?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// Inline progress (no newline) — caller appends [OK]/[ERROR] afterward
macro_rules! log_progress {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Cyan)))?;
        write!($out, $($arg)*)?;
        $out.flush()?;
        $out.reset()?;
    }};
}

/// Inline colored text (no newline) — for building up a line piece by piece
macro_rules! log_write {
    ($out:expr, $color:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some($color)))?;
        write!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// Inline suffix [OK] in Green (newline after)
macro_rules! log_ok_suffix {
    ($out:expr) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Green)))?;
        writeln!($out, " [OK]")?;
        $out.reset()?;
    }};
}

/// Inline suffix [ERROR] in Red (newline after)
macro_rules! log_error_suffix {
    ($out:expr) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Red)))?;
        writeln!($out, " [ERROR]")?;
        $out.reset()?;
    }};
}
