/// [INFO] message in Cyan (with newline)
macro_rules! log_info {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Cyan)))?;
        write!($out, "[INFO] ")?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// [OK] message in Green (with newline)
macro_rules! log_ok {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Green)))?;
        write!($out, "[OK] ")?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// [WARN] message in Yellow (with newline)
macro_rules! log_warn {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Yellow)))?;
        write!($out, "[WARN] ")?;
        writeln!($out, $($arg)*)?;
        $out.reset()?;
    }};
}

/// [ERROR] message in Red (with newline)
macro_rules! log_error {
    ($out:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some(::termcolor::Color::Red)))?;
        write!($out, "[ERROR] ")?;
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

/// Inline colored text (no newline) — for building up a line piece by piece
macro_rules! log_write {
    ($out:expr, $color:expr, $($arg:tt)*) => {{
        $out.set_color(::termcolor::ColorSpec::new().set_fg(Some($color)))?;
        write!($out, $($arg)*)?;
        $out.reset()?;
    }};
}
