use crate::core;
use crate::util;
use crate::util::attach::{self, Attachment};
use crate::util::split_args;
use crate::util::terminal;
use rustyline::Context;
use rustyline::Helper;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use std::fs;
use std::io::Write;

use super::eval::{eval_expr, format_eval_error};
use super::parse::strip_inline_commands;

const HELP_TEXT: &str = "\nCommands:\n\
/help  Show this help message\n\
/clean Clear chat history\n\
/add   Attach file contents to chat context\n\
/eval  Evaluate arithmetic expression\n\
/save  Save a summary of the chat\n\
/stream [on|off] Toggle streaming output\n";

/// Provides command name completions for slash-prefixed commands in the prompt.
pub struct CommandCompleter {
    /// The set of slash commands available for completion.
    commands: Vec<&'static str>,
    /// Filename completer for /add paths.
    file_completer: FilenameCompleter,
    /// Common command names for #!(...) completion.
    inline_commands: Vec<&'static str>,
    /// Common git subcommands for #!(git ...) completion.
    git_subcommands: Vec<&'static str>,
}

impl CommandCompleter {
    pub fn new(commands: Vec<&'static str>) -> Self {
        Self {
            commands,
            file_completer: FilenameCompleter::new(),
            inline_commands: vec![
                "ls", "cat", "rg", "git", "pwd", "grep", "sed", "awk", "head", "tail",
            ],
            git_subcommands: vec![
                "status", "add", "commit", "push", "pull", "fetch", "log", "diff", "show",
                "branch", "checkout", "switch", "merge", "rebase", "stash", "reset", "restore",
            ],
        }
    }
}

/// Enables rustyline helper integration for slash command completion.
impl Helper for CommandCompleter {}
/// Disables hints while still fulfilling the rustyline helper contract.
impl Hinter for CommandCompleter {
    type Hint = String;

    /// Returns no hint so user input remains unobstructed.
    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

/// Disables highlighting while still fulfilling the rustyline helper contract.
impl Highlighter for CommandCompleter {}

/// Disables validation while still fulfilling the rustyline helper contract.
impl Validator for CommandCompleter {}

/// Implements slash command completion for rustyline.
impl Completer for CommandCompleter {
    type Candidate = Pair;

    /// Returns completions when the current token starts with `/`.
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        if line.starts_with("/add ") && pos >= 5 {
            return self.file_completer.complete(line, pos, ctx);
        }
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let token = &line[start..pos];
        if let Some(inline_start) = find_inline_start(&line[..pos])
            && is_inside_inline(&line[..pos], inline_start) {
                let inline_slice = &line[inline_start..pos];
                let token_start = inline_slice
                    .rfind(|c: char| c.is_whitespace())
                    .map(|idx| inline_start + idx + 1)
                    .unwrap_or(inline_start);
                let inline_token = &line[token_start..pos];
                if util::looks_like_path(inline_token) {
                    return self.file_completer.complete(line, pos, ctx);
                }
                if token_start == inline_start {
                    let matches = self
                        .inline_commands
                        .iter()
                        .filter(|cmd| cmd.starts_with(inline_token))
                        .map(|cmd| Pair {
                            display: cmd.to_string(),
                            replacement: cmd.to_string(),
                        })
                        .collect();
                    return Ok((token_start, matches));
                }
                if let Some((first, first_end)) = first_inline_token(line, inline_start, pos)
                    && first == "git" && token_start > first_end {
                        let matches = self
                            .git_subcommands
                            .iter()
                            .filter(|cmd| cmd.starts_with(inline_token))
                            .map(|cmd| Pair {
                                display: cmd.to_string(),
                                replacement: cmd.to_string(),
                            })
                            .collect();
                        return Ok((token_start, matches));
                    }
            }
        if start == 0 && token.starts_with('/') {
            let matches: Vec<Pair> = self
                .commands
                .iter()
                .filter(|cmd| cmd.starts_with(token))
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                })
                .collect();
            if !matches.is_empty() || token == "/" {
                return Ok((start, matches));
            }
        }
        if util::looks_like_path(token) {
            return self.file_completer.complete(line, pos, ctx);
        }

        Ok((pos, Vec::new()))
    }
}

fn find_inline_start(input: &str) -> Option<usize> {
    input.rfind("#!(").map(|idx| idx + 3)
}

fn is_inside_inline(input: &str, start: usize) -> bool {
    let mut depth = 1;
    for ch in input[start..].chars() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return false;
            }
        }
    }
    true
}

fn first_inline_token(input: &str, start: usize, end: usize) -> Option<(&str, usize)> {
    let slice = &input[start..end];
    let trimmed = slice.trim_start();
    let leading_ws = slice.len() - trimmed.len();
    let token_start = start + leading_ws;
    let mut iter = trimmed.split_whitespace();
    let first = iter.next()?;
    let first_end = token_start + first.len();
    Some((first, first_end))
}

pub fn handle_help(user_input: &str) -> bool {
    if user_input == "/help" {
        println!("{HELP_TEXT}");
        return true;
    }
    false
}

pub fn handle_clean(user_input: &str, history: &mut Vec<String>) -> bool {
    if user_input == "/clean" {
        history.clear();
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
        return true;
    }
    false
}

pub fn handle_stream(user_input: &str, stream_enabled: &mut bool) -> bool {
    let Some(rest) = user_input.strip_prefix("/stream") else {
        return false;
    };
    let mode = rest.trim().to_lowercase();
    if mode == "on" {
        *stream_enabled = true;
        println!("\nstream: on");
    } else if mode == "off" {
        *stream_enabled = false;
        println!("\nstream: off");
    } else {
        println!("\nUsage: /stream on|off");
    }
    true
}

pub fn handle_add(
    user_input: &str,
    history: &mut Vec<String>,
    pending_stdin: &mut Option<String>,
) -> bool {
    let Some(rest) = user_input.strip_prefix("/add") else {
        return false;
    };
    let args = split_args(rest.trim());
    if args.is_empty() {
        println!("\nUsage: /add <path> [path2 path3 ...]");
        return true;
    }

    let mut attachments = Vec::new();
    for path in args {
        match fs::read_to_string(&path) {
            Ok(content) => {
                history.push(format!("Attachment: {}\n{}\n", path, content));
                println!("\nadded: {}", path);
                attachments.push(Attachment { path, content });
            }
            Err(err) => {
                eprintln!("\nError reading {}: {}", path, err);
            }
        }
    }

    if let Some(block) = attach::format_attachments(&attachments) {
        *pending_stdin = Some(match pending_stdin.take() {
            Some(existing) => existing + &block,
            None => block,
        });
    }
    true
}

pub fn handle_eval(user_input: &str) -> bool {
    let Some(rest) = user_input.strip_prefix("/eval") else {
        return false;
    };
    let expr = strip_inline_commands(rest).trim().to_string();
    if expr.is_empty() {
        println!("\nUsage: /eval <expression>");
        return true;
    }

    match eval_expr(&expr) {
        Ok(value) => println!("\n{}", value),
        Err(err) => println!("\nError: {}", format_eval_error(err)),
    }

    true
}

pub async fn handle_save(
    user_input: &str,
    service: &core::Service,
    args: &core::Cli,
    history: &[String],
) -> Result<bool, String> {
    let Some(rest) = user_input.strip_prefix("/save") else {
        return Ok(false);
    };
    let raw_text = strip_inline_commands(rest).trim().to_string();

    let datetime = util::current_datetime();
    let history_text = history.join("\n");
    let prompt = if raw_text.is_empty() {
        format!(
            "Write a summary for the user.\n\
Use the same language as the user.\n\
Do not add footers, notes, or meta commentary.\n\
Chat history:\n\
{history_text}\n"
        )
    } else {
        format!(
            "Hint (required): {raw_text}\n\
Chat history:\n\
{history_text}\n"
        )
    };

    if args.verbose {
        terminal::print_verbose(&prompt);
    }

    let result = match service.complete(&prompt).await {
        Ok(text) => text,
        Err(err) => return Err(format!("AI error: {}", err)),
    };

    let output = result.trim_end().to_string();
    let safe_datetime = datetime.replace(' ', ".").replace(':', "_");
    let filename = format!("gareth.{}.md", safe_datetime);
    let path = filename.as_str();
    let write_result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, output.as_bytes()));

    match write_result {
        Ok(()) => {
            println!("\nsaved: {}", path);
            Ok(true)
        }
        Err(err) => Err(format!("File error: {}", err)),
    }
}
