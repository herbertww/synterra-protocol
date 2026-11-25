# Terminal colours

The colour spec for everything the `synterra` binary prints: `check`, `list`, `info`, `verify` and the `run` log. It is written to be implemented directly.

## When to colour

Decide once per output stream (stdout and stderr separately). The first rule that matches wins.

1. `--color=never` means off. `--color=always` means on.
2. `NO_COLOR` set to a non-empty value means off ([no-color.org](https://no-color.org)). An explicit `--color=always` still wins, as the convention asks.
3. `CLICOLOR_FORCE` set, non-empty and not `0`, means on.
4. The stream is not a terminal (`std::io::IsTerminal`) means off. Pipes, files and CI logs get plain text.
5. `TERM=dumb` means off.
6. On Windows, on only when `WT_SESSION` (Windows Terminal) or `TERM` (mintty, Git Bash) is set. A legacy console prints raw escape codes unless the program first enables VT processing with `SetConsoleMode(ENABLE_VIRTUAL_TERMINAL_PROCESSING)`. Either enable it and colour, or stay plain.
7. Otherwise on.

**Depth:** if `COLORTERM` is `truecolor` or `24bit`, use the 24-bit column. Otherwise use the 16-colour column, which lets the user's terminal theme pick the actual shade.

## The table

Every coloured span ends with a reset, `ESC[0m`. Write SGR codes as `ESC[<code>m`, where `ESC` is `\x1b`.

| Role | Where it appears | Hex | 24-bit SGR | 16-colour SGR |
|---|---|---|---|---|
| **pass** | the gate status word `pass` | `#5677EC` | `38;2;86;119;236` | `36` (cyan) |
| **fail** | the gate status word `fail`; error lines | `#E5484D` | `38;2;229;72;77` | `31` (red) |
| **skipped** | the gate status word `skipped` | `#7F7D79` | `38;2;127;125;121` | `2` (faint) |
| **advisory** | the trailing ` (advisory)` on a gate row | `#A87019` | `38;2;168;112;25` | `33` (yellow) |
| **accepted chip** | ` ACCEPTED ` after `verdict` | `#F1EFEA` on `#2347C6` | `1;38;2;241;239;234;48;2;35;71;198` | `1;97;44` |
| **rejected chip** | ` REJECTED ` after `verdict` | `#F1EFEA` on `#B3261E` | `1;38;2;241;239;234;48;2;179;38;30` | `1;97;41` |
| **id** | 64-hex object ids, signer keys, short ids | `#7F7D79` | `38;2;127;125;121` | `2` (faint) |
| **demo label** | the `[demo: …]` bracket on the `score` line | `#7F7D79` | `38;2;127;125;121` | `2` (faint) |
| **warning** | `WARNING:` lines; `not signed: …` | `#A87019`, bold | `1;38;2;168;112;25` | `1;33` |
| **plain** | gate names, details, numbers, headings | terminal default | none | none |

## Contrast, measured

WCAG contrast ratios, computed from the hex values. The foregrounds are mid-tones chosen to hold about 4:1 on both light and dark terminals, because a CLI cannot reliably know its background.

| Colour | on `#FFFFFF` (light) | on `#F1EFEA` (paper) | on `#0C0C0C` (Windows Terminal) | on `#1E1E1E` (VS Code) | on `#000000` |
|---|---|---|---|---|---|
| pass `#5677EC` | 4.00 | 3.48 | 4.89 | 4.16 | 5.25 |
| fail `#E5484D` | 3.91 | 3.41 | 5.00 | 4.26 | 5.37 |
| skipped / id `#7F7D79` | 4.11 | 3.57 | 4.76 | 4.06 | 5.11 |
| advisory `#A87019` | 4.20 | 3.66 | 4.65 | 3.97 | 5.00 |

The verdict chips carry their own background, so they read the same on every theme: `#F1EFEA` on `#2347C6` is 6.55:1, and `#F1EFEA` on `#B3261E` is 5.69:1.

**Why pass falls back to cyan, not blue:** ANSI blue (`34`) is unreadable on the default dark themes of common consoles. Windows Terminal's Campbell blue `#0037DA` on its `#0C0C0C` background is 2.38:1, and xterm's `#0000EE` on black is 2.23:1. Campbell cyan `#3A96DD` on the same background is 6.14:1, and cyan is the nearest legible relative of cobalt.

## Layout rules

- Pad before you colour. Compute column widths on the plain word (`pass`, `skipped`), then wrap the padded text in SGR. Escape codes have no width, and padding a coloured string breaks the columns.
- The gate row is `{name:<14} {status:<8} {detail}{advisory}`. Only the status word and the ` (advisory)` suffix are coloured. The detail stays plain.
- The verdict row is `verdict        ` plus the chip. The chip text is the verdict in capitals with one space inside each end: ` ACCEPTED `, ` REJECTED `. Capitals appear only in the chip, because it is the stamp.
- The attestation row is `attestation    ` plus the id in the id colour.
- Never use colour as the only signal. The words `pass`, `fail`, `skipped`, `(advisory)`, `ACCEPTED` and `REJECTED` always print, so colour-blind users and plain logs lose nothing.
- Use no blinking, no italics (many terminals lack them) and no background colour except on the two chips.

## Reference implementation sketch (Rust)

```rust
pub struct Style { pub truecolor: &'static str, pub ansi16: &'static str }

pub const PASS: Style     = Style { truecolor: "38;2;86;119;236",  ansi16: "36" };
pub const FAIL: Style     = Style { truecolor: "38;2;229;72;77",   ansi16: "31" };
pub const SKIPPED: Style  = Style { truecolor: "38;2;127;125;121", ansi16: "2" };
pub const ADVISORY: Style = Style { truecolor: "38;2;168;112;25",  ansi16: "33" };
pub const ID: Style       = SKIPPED;
pub const WARNING: Style  = Style { truecolor: "1;38;2;168;112;25", ansi16: "1;33" };
pub const ACCEPTED: Style = Style { truecolor: "1;38;2;241;239;234;48;2;35;71;198", ansi16: "1;97;44" };
pub const REJECTED: Style = Style { truecolor: "1;38;2;241;239;234;48;2;179;38;30", ansi16: "1;97;41" };

#[derive(Clone, Copy, PartialEq)]
pub enum Depth { Off, Ansi16, TrueColor }

pub fn paint(depth: Depth, s: &Style, text: &str) -> String {
    match depth {
        Depth::Off => text.to_string(),
        Depth::Ansi16 => format!("\x1b[{}m{}\x1b[0m", s.ansi16, text),
        Depth::TrueColor => format!("\x1b[{}m{}\x1b[0m", s.truecolor, text),
    }
}
```

A gate row then prints as `format!("{:<14} {} {}{}", name, paint(d, &PASS, &format!("{:<8}", "pass")), detail, adv)`, with `adv` set to `paint(d, &ADVISORY, " (advisory)")` or empty.
