//! The line-based scenario format: `tap id:next`, `expect Count: 1`, `#` for
//! comments. Commands wait on their own so a script reads as a flat sequence.
//!
//! Anything the DSL cannot express becomes a Rust test against the driver —
//! no special-case commands are added for it.

use std::path::Path;

use crate::bundle;
use crate::driver::{Selector, Session};

#[derive(Clone, Debug, PartialEq)]
enum Step {
    /// Optional first directive naming the fixture app, `app counter`.
    App(String),
    Tap(Selector),
    Type(Option<Selector>, String),
    Scroll(Selector, f32, f32),
    Expect(String),
    Absent(String),
    Eval(String),
    PostEval(String),
    HostReply(String),
    Visible(bool),
    ExpectFaults(u32),
    ExpectStatus(String),
}

/// Parse one `*.script` file. Syntax errors carry the line number.
fn parse(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let step = parse_step(line)
            .ok_or_else(|| format!("line {}: cannot parse {line:?}", index + 1))?;
        steps.push(step);
    }
    Ok(steps)
}

fn parse_step(line: &str) -> Option<Step> {
    let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
    let rest = rest.trim();
    match head {
        "app" => Some(Step::App(rest.to_string())),
        "tap" => Some(Step::Tap(parse_selector(rest)?)),
        "type" => {
            if let Some(quoted) = quoted(rest) {
                return Some(Step::Type(None, unquote(quoted)?));
            }
            let (selector, tail) = rest.split_once(' ')?;
            let text = quoted(tail.trim())?;
            Some(Step::Type(Some(parse_selector(selector)?), unquote(text)?))
        }
        "scroll" => {
            let mut parts = rest.split_whitespace();
            let selector = parse_selector(parts.next()?)?;
            let dx = parts.next()?.parse().ok()?;
            let dy = parts.next()?.parse().ok()?;
            Some(Step::Scroll(selector, dx, dy))
        }
        "expect" => Some(Step::Expect(rest.to_string())),
        "absent" => Some(Step::Absent(rest.to_string())),
        "eval" => Some(Step::Eval(eval_source(rest))),
        "post-eval" => Some(Step::PostEval(eval_source(rest))),
        "host-reply" => Some(Step::HostReply(rest.to_string())),
        "visible" => Some(Step::Visible(match rest {
            "on" => true,
            "off" => false,
            _ => return None,
        })),
        "expect-faults" => {
            let (op, count) = rest.split_once(' ')?;
            if op != ">=" {
                return None;
            }
            Some(Step::ExpectFaults(count.parse().ok()?))
        }
        "expect-status" => Some(Step::ExpectStatus(rest.to_string())),
        _ => None,
    }
}

/// `eval` accepts either a quoted string or bare source.
fn eval_source(text: &str) -> String {
    quoted(text).map_or_else(|| text.to_string(), |source| unquote(source).expect("balanced quotes"))
}

fn parse_selector(text: &str) -> Option<Selector> {
    if let Some(id) = text.strip_prefix("id:") {
        return Some(Selector::Id(id.to_string()));
    }
    if let Some(quoted) = text.strip_prefix("text:") {
        return Some(Selector::Text(unquote(quoted)?));
    }
    None
}

/// `"…"` with the quotes still on, so `expect` free text stays unquoted.
fn quoted(text: &str) -> Option<&str> {
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        Some(text)
    } else {
        None
    }
}

fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

/// Run one parsed script against its fixture app.
fn run(name: &str, steps: &[Step]) -> Result<(), String> {
    let app_name = steps.iter().find_map(|step| match step {
        Step::App(name) => Some(name.clone()),
        _ => None,
    });
    let app = bundle::app(app_name.as_deref().unwrap_or(name))?;
    let session = Session::start(&app)?;
    for (index, step) in steps.iter().enumerate() {
        let line = index + 1;
        run_step(&session, step)
            .map_err(|error| format!("line {line}: {error}"))?;
    }
    Ok(())
}

fn run_step(session: &Session, step: &Step) -> Result<(), String> {
    match step {
        Step::App(_) => Ok(()),
        Step::Tap(selector) => session.tap(selector),
        Step::Type(selector, text) => match selector {
            Some(selector) => session.type_text(selector, text),
            None => {
                let selector = Selector::Text(text.clone());
                session.type_text(&selector, text)
            }
        },
        Step::Scroll(selector, dx, dy) => session.scroll(selector, *dx, *dy),
        Step::Expect(text) => session.wait_for(text),
        Step::Absent(text) => session.wait_absent(text),
        Step::Eval(source) => session.eval(source).map(|_| ()),
        Step::PostEval(source) => session.post_eval(source),
        Step::HostReply(result) => session.reply_next_request(result),
        Step::Visible(visible) => {
            session.set_visible(*visible);
            Ok(())
        }
        Step::ExpectFaults(count) => session.wait_faults(*count),
        Step::ExpectStatus(status) => session.wait_status(status),
    }
}

/// Parse and run one script file. `name` is the file stem, used to pick the
/// fixture app when the script has no `app` directive.
pub fn run_file(path: &Path) -> Result<(), String> {
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{}: script has no file stem", path.display()))?
        .to_string();
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    run(&name, &parse(&text)?)
}
