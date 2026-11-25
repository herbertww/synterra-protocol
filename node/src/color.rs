//! Terminal colour. The palette is the CounterBank brand kit (ink, paper, cobalt), checked for
//! contrast on light and dark terminals. Colour is decoration only: every word means the same
//! without it, and it is off whenever output is not a terminal (or NO_COLOR is set).

use std::io::IsTerminal;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub enum Role {
    Pass,
    Fail,
    Dim,
    Advisory,
    Warn,
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, PartialEq)]
enum Depth {
    Off,
    Ansi16,
    True,
}

static DEPTH: OnceLock<Depth> = OnceLock::new();

/// `choice` is the --color flag: "always", "never" or "auto".
pub fn init(choice: &str) {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let on = match choice {
        "always" => true,
        "never" => false,
        _ => {
            if env("NO_COLOR").is_some() {
                false
            } else if env("CLICOLOR_FORCE").map(|v| v != "0").unwrap_or(false) {
                true
            } else if !std::io::stdout().is_terminal() || env("TERM").as_deref() == Some("dumb") {
                false
            } else if cfg!(windows) {
                env("WT_SESSION").is_some() || env("TERM").is_some()
            } else {
                true
            }
        }
    };
    let truecolor = matches!(
        env("COLORTERM").as_deref(),
        Some("truecolor") | Some("24bit")
    );
    let _ = DEPTH.set(if !on {
        Depth::Off
    } else if truecolor {
        Depth::True
    } else {
        Depth::Ansi16
    });
}

fn sgr(role: Role, depth: Depth) -> &'static str {
    match (role, depth == Depth::True) {
        (Role::Pass, true) => "38;2;86;119;236",
        (Role::Pass, false) => "36",
        (Role::Fail, true) => "38;2;229;72;77",
        (Role::Fail, false) => "31",
        (Role::Dim, true) => "38;2;127;125;121",
        (Role::Dim, false) => "2",
        (Role::Advisory, true) => "38;2;168;112;25",
        (Role::Advisory, false) => "33",
        (Role::Warn, true) => "1;38;2;168;112;25",
        (Role::Warn, false) => "1;33",
        (Role::Accepted, true) => "1;38;2;241;239;234;48;2;35;71;198",
        (Role::Accepted, false) => "1;97;44",
        (Role::Rejected, true) => "1;38;2;241;239;234;48;2;179;38;30",
        (Role::Rejected, false) => "1;97;41",
    }
}

/// Colour `text` (already padded to its column width) for `role`.
pub fn paint(role: Role, text: &str) -> String {
    match DEPTH.get().copied().unwrap_or(Depth::Off) {
        Depth::Off => text.to_string(),
        d => format!("\x1b[{}m{text}\x1b[0m", sgr(role, d)),
    }
}

/// A verdict chip: ` ACCEPTED ` / ` REJECTED `, or the plain word without colour.
pub fn chip(accepted: bool, word: &str) -> String {
    match DEPTH.get().copied().unwrap_or(Depth::Off) {
        Depth::Off => word.to_string(),
        _ => paint(
            if accepted {
                Role::Accepted
            } else {
                Role::Rejected
            },
            &format!(" {} ", word.to_uppercase()),
        ),
    }
}

/// A gate status word, padded to `width` before colouring so columns stay aligned.
pub fn status(s: crate::model::GateStatus, width: usize) -> String {
    use crate::model::GateStatus::*;
    let word = format!("{:<width$}", format!("{s:?}").to_lowercase());
    match s {
        Pass => paint(Role::Pass, &word),
        Fail => paint(Role::Fail, &word),
        Skipped => paint(Role::Dim, &word),
    }
}
