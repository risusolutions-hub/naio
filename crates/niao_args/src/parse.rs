use std::collections::HashMap;
use std::ffi::OsString;

use crate::arg::{Arg, ArgAction};
use crate::command::Command;
use crate::error::{Error, ErrorKind};
use crate::help;
use crate::matches::ArgMatches;
use crate::value::ValueSource;

#[derive(Debug, Clone)]
struct ArgLookup<'a> {
    by_long: HashMap<String, &'a Arg>,
    by_short: HashMap<char, &'a Arg>,
    positionals: Vec<&'a Arg>,
    id_map: HashMap<String, &'a Arg>,
}

fn build_lookup(cmd: &Command) -> ArgLookup<'_> {
    let mut by_long = HashMap::new();
    let mut by_short = HashMap::new();
    let mut positionals = Vec::new();
    let mut id_map = HashMap::new();

    for arg in &cmd.args {
        if arg.long.as_deref() == Some("") || arg.long.is_none() && arg.short.is_none() {
            positionals.push(arg);
        } else {
            if let Some(ref long) = arg.long {
                if !long.is_empty() {
                    by_long.insert(long.clone(), arg);
                }
            }
            if let Some(short) = arg.short {
                by_short.insert(short, arg);
            }
        }
        id_map.insert(arg.id.clone(), arg);
    }

    ArgLookup {
        by_long,
        by_short,
        positionals,
        id_map,
    }
}

pub fn parse_command<I, T>(cmd: &Command, itr: I) -> Result<ArgMatches, Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut raw: Vec<OsString> = itr.into_iter().map(Into::into).collect();
    if raw.is_empty() {
        raw.push(OsString::from(""));
    }

    let bin = raw[0].clone();
    let bin_name = cmd
        .bin_name
        .as_deref()
        .or_else(|| raw[0].to_str())
        .unwrap_or(&cmd.name)
        .to_string();

    let args = &raw[1..];

    if cmd.arg_required_else_help && args.is_empty() && !cmd.subcommands.is_empty() {
        return Err(Error::display_help(help::render_help(cmd)));
    }

    if args.is_empty() && cmd.subcommands.is_empty() && cmd.arg_required_else_help {
        return Err(Error::display_help(help::render_help(cmd)));
    }

    if let Some((sub_name, sub_cmd, rest)) = match_subcommand(cmd, args) {
        let child = parse_command(
            sub_cmd,
            std::iter::once(bin.clone()).chain(rest.iter().cloned()),
        )?;
        let mut outer = ArgMatches {
            name: bin_name,
            ..Default::default()
        };
        outer.subcommand = Some(Box::new((sub_name, child)));
        return Ok(outer);
    }

    parse_flat(cmd, bin_name, args)
}

fn match_subcommand<'a>(
    cmd: &'a Command,
    args: &[OsString],
) -> Option<(String, &'a Command, Vec<OsString>)> {
    if cmd.subcommands.is_empty() || args.is_empty() {
        return None;
    }
    let first = args[0].to_string_lossy();
    for sub in &cmd.subcommands {
        if sub.name == first.as_ref() {
            return Some((sub.name.clone(), sub, args[1..].to_vec()));
        }
        for alias in &sub.visible_aliases {
            if alias == first.as_ref() {
                return Some((sub.name.clone(), sub, args[1..].to_vec()));
            }
        }
        if let Some(canonical) = sub.aliases.get(first.as_ref()) {
            return Some((canonical.clone(), sub, args[1..].to_vec()));
        }
    }
    None
}

fn advance_pos(pos_idx: usize, pdef: &Arg) -> usize {
    match pdef.num_args {
        crate::arg::NumArgs::ZeroOrMore | crate::arg::NumArgs::OneOrMore => pos_idx,
        _ => pos_idx + 1,
    }
}

fn parse_flat(cmd: &Command, bin_name: String, args: &[OsString]) -> Result<ArgMatches, Error> {
    let lookup = build_lookup(cmd);
    let mut matches = ArgMatches {
        name: bin_name,
        ..Default::default()
    };

    apply_defaults(cmd, &mut matches);

    let mut idx = 0usize;
    let mut pos_idx = 0usize;
    let mut trailing = false;

    while idx < args.len() {
        let arg = &args[idx];
        let s = arg.to_string_lossy();

        if trailing {
            let pidx = lookup
                .positionals
                .iter()
                .position(|a| a.trailing_var_arg)
                .unwrap_or_else(|| pos_idx.saturating_sub(1));
            push_positional(&lookup, &mut matches, pidx, arg.clone())?;
            idx += 1;
            continue;
        }

        if s == "--" {
            idx += 1;
            trailing = true;
            if let Some(p) = lookup.positionals.iter().position(|a| a.trailing_var_arg) {
                pos_idx = p;
            }
            continue;
        }

        if s == "--help" || s == "-h" {
            if lookup.by_long.contains_key("help") {
                return Err(Error::display_help(help::render_help(cmd)));
            }
        }

        if s == "--version" {
            if lookup.by_long.contains_key("version") {
                if let Some(ver) = cmd.version.as_deref() {
                    return Err(Error::new(ErrorKind::DisplayVersion, ver.to_string()));
                }
            }
        }

        if s.starts_with("--") {
            let body = &s[2..];
            let (name, value) = if let Some((n, v)) = body.split_once('=') {
                (n, Some(v))
            } else {
                (body, None)
            };

            let arg_def = lookup.by_long.get(name).ok_or_else(|| unknown(&s))?;

            idx = consume_option(arg_def, value, args, idx, &mut matches)?;
            if arg_def.trailing_var_arg {
                trailing = true;
            }
            continue;
        }

        if s.starts_with('-') && s.len() > 1 {
            let shorts = s[1..].chars().collect::<Vec<_>>();
            for (i, ch) in shorts.iter().enumerate() {
                let arg_def = lookup
                    .by_short
                    .get(ch)
                    .ok_or_else(|| unknown(&format!("-{ch}")))?;

                if arg_def.action == ArgAction::SetTrue || arg_def.action == ArgAction::Count {
                    apply_flag(arg_def, &mut matches);
                    continue;
                }

                let inline = if i + 1 == shorts.len() {
                    None
                } else {
                    Some(shorts[i + 1..].iter().collect::<String>())
                };

                if let Some(v) = inline {
                    consume_value(arg_def, OsString::from(v), &mut matches)?;
                } else if idx + 1 < args.len() {
                    idx += 1;
                    consume_value(arg_def, args[idx].clone(), &mut matches)?;
                } else {
                    return Err(missing_value(arg_def));
                }
            }
            idx += 1;
            continue;
        }

        // positional
        if pos_idx < lookup.positionals.len() {
            let pdef = lookup.positionals[pos_idx];
            if pdef.trailing_var_arg {
                for rest in &args[idx..] {
                    push_positional(&lookup, &mut matches, pos_idx, rest.clone())?;
                }
                break;
            }
            push_positional(&lookup, &mut matches, pos_idx, arg.clone())?;
            if pdef.trailing_var_arg {
                trailing = true;
            } else if pdef.allow_hyphen_values {
                trailing = true;
            } else {
                pos_idx = advance_pos(pos_idx, pdef);
            }
            idx += 1;
            continue;
        }

        // additional values for ZeroOrMore / OneOrMore positionals
        if let Some(pdef) = lookup.positionals.last() {
            if matches!(
                pdef.num_args,
                crate::arg::NumArgs::ZeroOrMore | crate::arg::NumArgs::OneOrMore
            ) {
                push_positional(
                    &lookup,
                    &mut matches,
                    lookup.positionals.len() - 1,
                    arg.clone(),
                )?;
                idx += 1;
                continue;
            }
        }

        return Err(unknown(&s));
    }

    validate_required(cmd, &lookup, &matches)?;
    Ok(matches)
}

fn consume_option(
    arg_def: &Arg,
    inline: Option<&str>,
    args: &[OsString],
    idx: usize,
    matches: &mut ArgMatches,
) -> Result<usize, Error> {
    match arg_def.action {
        ArgAction::SetTrue => {
            apply_flag(arg_def, matches);
            Ok(idx + 1)
        }
        ArgAction::Count => {
            let n = matches.get_count(&arg_def.id) as usize + 1;
            matches.set_values(
                &arg_def.id,
                vec![OsString::from(n.to_string())],
                ValueSource::CommandLine,
            );
            Ok(idx + 1)
        }
        ArgAction::Set | ArgAction::Append => {
            if let Some(v) = inline {
                consume_value(arg_def, OsString::from(v), matches)?;
                Ok(idx + 1)
            } else if idx + 1 < args.len() {
                consume_value(arg_def, args[idx + 1].clone(), matches)?;
                Ok(idx + 2)
            } else {
                Err(missing_value(arg_def))
            }
        }
    }
}

fn consume_value(arg_def: &Arg, raw: OsString, matches: &mut ArgMatches) -> Result<(), Error> {
    if let Some(delim) = arg_def.value_delimiter {
        let s = raw.to_string_lossy();
        let parts: Vec<OsString> = s
            .split(delim)
            .filter(|p| !p.is_empty())
            .map(OsString::from)
            .collect();
        if arg_def.action == ArgAction::Append {
            for part in parts {
                matches.push_value(&arg_def.id, part, ValueSource::CommandLine);
            }
        } else {
            matches.set_values(&arg_def.id, parts, ValueSource::CommandLine);
        }
        return Ok(());
    }

    match arg_def.action {
        ArgAction::Append => {
            matches.push_value(&arg_def.id, raw, ValueSource::CommandLine);
        }
        _ => {
            matches.set_values(&arg_def.id, [raw], ValueSource::CommandLine);
        }
    }
    Ok(())
}

fn apply_flag(arg_def: &Arg, matches: &mut ArgMatches) {
    match arg_def.action {
        ArgAction::Count => {
            let n = matches.get_count(&arg_def.id) as usize + 1;
            matches.set_values(
                &arg_def.id,
                vec![OsString::from(n.to_string())],
                ValueSource::CommandLine,
            );
        }
        _ => {
            matches.set_flag(&arg_def.id, true, ValueSource::CommandLine);
        }
    }
}

fn push_positional(
    lookup: &ArgLookup<'_>,
    matches: &mut ArgMatches,
    pos_idx: usize,
    value: OsString,
) -> Result<(), Error> {
    let arg_def = lookup.positionals.get(pos_idx).ok_or_else(|| {
        Error::new(
            ErrorKind::TooManyValues {
                arg: "positional".to_string(),
            },
            "unexpected extra positional argument",
        )
    })?;
    matches.push_value(&arg_def.id, value, ValueSource::CommandLine);
    Ok(())
}

fn apply_defaults(cmd: &Command, matches: &mut ArgMatches) {
    for arg in &cmd.args {
        if let Some(ref env) = arg.env {
            if let Ok(val) = std::env::var(env.to_string_lossy().as_ref()) {
                if arg.action == ArgAction::SetTrue {
                    if val == "1"
                        || val.eq_ignore_ascii_case("true")
                        || val.eq_ignore_ascii_case("yes")
                    {
                        matches.set_flag(&arg.id, true, ValueSource::EnvVariable);
                    }
                } else if !arg.default_values.is_empty() {
                    matches.set_values(
                        &arg.id,
                        arg.default_values.iter().cloned(),
                        ValueSource::EnvVariable,
                    );
                } else {
                    matches.push_value(&arg.id, OsString::from(val), ValueSource::EnvVariable);
                }
                continue;
            }
        }

        if arg.action == ArgAction::SetTrue {
            matches.set_flag(&arg.id, false, ValueSource::DefaultValue);
        }

        if !arg.default_values.is_empty() {
            matches.set_values(
                &arg.id,
                arg.default_values.iter().cloned(),
                ValueSource::DefaultValue,
            );
        } else if let Some(ref dv) = arg.default_value {
            if arg.action == ArgAction::SetTrue {
                let s = dv.to_string_lossy();
                if s == "true" || s == "1" {
                    matches.set_flag(&arg.id, true, ValueSource::DefaultValue);
                }
            } else {
                matches.push_value(&arg.id, dv.clone(), ValueSource::DefaultValue);
            }
        }
    }
}

fn validate_required(
    cmd: &Command,
    lookup: &ArgLookup<'_>,
    matches: &ArgMatches,
) -> Result<(), Error> {
    for arg in &cmd.args {
        if !arg.required {
            continue;
        }
        let has = match arg.action {
            ArgAction::SetTrue => matches.get_flag(&arg.id),
            _ => matches
                .values
                .get(&arg.id)
                .map(|v| !v.is_empty())
                .unwrap_or(false),
        };
        if !has {
            return Err(Error::new(
                ErrorKind::MissingRequiredArgument,
                format!(
                    "the following required arguments were not provided: <{}>",
                    arg.id
                ),
            ));
        }
    }

    if !cmd.subcommands.is_empty() && lookup.positionals.is_empty() {
        // subcommand required at this level only when no positionals and no subcommand selected
    }

    Ok(())
}

fn unknown(arg: &str) -> Error {
    Error::new(
        ErrorKind::UnknownArgument,
        format!("unexpected argument '{arg}' found"),
    )
}

fn missing_value(arg: &Arg) -> Error {
    let name = if let Some(ref long) = arg.long {
        if !long.is_empty() {
            format!("--{long}")
        } else {
            arg.id.clone()
        }
    } else if let Some(c) = arg.short {
        format!("-{c}")
    } else {
        arg.id.clone()
    };
    Error::new(
        ErrorKind::MissingRequiredArgument,
        format!("a value is required for '{name}' but none was supplied"),
    )
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::Arg;

    #[test]
    fn basic_flags_and_positionals() {
        let cmd = Command::new("app")
            .arg(Arg::long_flag("verbose", "verbose").short('v'))
            .arg(Arg::positional("file"));
        let m = cmd
            .try_get_matches_from(["app", "-v", "main.niao"])
            .unwrap();
        assert!(m.get_flag("verbose"));
        assert_eq!(m.get_one::<String>("file").as_deref(), Some("main.niao"));
    }

    #[test]
    fn long_eq_form() {
        let cmd = Command::new("app").arg(Arg::new("port").long("port"));
        let m = cmd.try_get_matches_from(["app", "--port=3000"]).unwrap();
        assert_eq!(m.get_one::<String>("port").as_deref(), Some("3000"));
    }

    #[test]
    fn nm_install_multi_lib() {
        let install = Command::new("install")
            .aliases(["i", "add"])
            .arg(
                Arg::new("libs")
                    .long("")
                    .value_name("LIB")
                    .num_args(crate::NumArgs::ZeroOrMore),
            )
            .arg(Arg::long_flag("global", "global"));
        let cmd = Command::new("nm").subcommand(install);
        let m = cmd
            .try_get_matches_from(["nm", "i", "json", "io", "--global"])
            .unwrap();
        let (_, inner) = m.subcommand().unwrap();
        assert!(inner.get_flag("global"));
        let libs: Vec<String> = inner.get_many("libs").unwrap().collect();
        assert_eq!(libs, vec!["json", "io"]);
    }

    #[test]
    fn run_double_dash_script_args() {
        let run = Command::new("run")
            .arg(Arg::positional("file"))
            .arg(
                Arg::new("args")
                    .long("")
                    .trailing_var_arg(true)
                    .allow_hyphen_values(true)
                    .num_args(crate::NumArgs::ZeroOrMore),
            )
            .arg(Arg::new("mode").long("mode").default_value("vm"));
        let cmd = Command::new("niao").subcommand(run);
        let m = cmd
            .try_get_matches_from(["niao", "run", "app.niao", "--", "foo", "-bar"])
            .unwrap();
        let (_, inner) = m.subcommand().unwrap();
        assert_eq!(inner.get_one::<String>("file").as_deref(), Some("app.niao"));
        let script: Vec<String> = inner.get_many("args").unwrap().collect();
        assert_eq!(script, vec!["foo", "-bar"]);
    }

    #[test]
    fn default_value() {
        let cmd = Command::new("app").arg(Arg::new("mode").long("mode").default_value("vm"));
        let m = cmd.try_get_matches_from(["app"]).unwrap();
        assert_eq!(m.get_one::<String>("mode").as_deref(), Some("vm"));
    }
}
