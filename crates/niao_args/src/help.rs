use crate::command::Command;

pub fn render_usage(cmd: &Command) -> String {
    let mut parts = Vec::with_capacity(8);
    parts.push(cmd.name.clone());
    if !cmd.subcommands.is_empty() {
        parts.push("<COMMAND>".to_string());
    }
    for arg in &cmd.args {
        if arg.long.as_deref() == Some("") || (arg.long.is_none() && arg.short.is_none()) {
            let name = arg.value_name.as_deref().unwrap_or(&arg.id).to_uppercase();
            if arg.required {
                parts.push(format!("<{name}>"));
            } else {
                parts.push(format!("[{name}]"));
            }
        }
    }
    format!("Usage: {}", parts.join(" "))
}

pub fn render_help(cmd: &Command) -> String {
    let mut out = String::with_capacity(512);
    out.push_str(&render_usage(cmd));
    out.push('\n');

    if let Some(about) = cmd.about.as_deref().or(cmd.long_about.as_deref()) {
        out.push('\n');
        out.push_str(about);
        out.push('\n');
    }

    if let Some(ver) = &cmd.version {
        out.push_str(&format!("\nVersion: {ver}\n"));
    }

    if !cmd.subcommands.is_empty() {
        out.push_str("\nCommands:\n");
        for sub in &cmd.subcommands {
            let mut line = format!("  {:<16}", sub.name);
            if let Some(about) = &sub.about {
                line.push_str(about);
            }
            out.push_str(&line);
            out.push('\n');
            for alias in &sub.visible_aliases {
                out.push_str(&format!("  {:<16}(alias: {alias})\n", ""));
            }
        }
    }

    let mut opts = Vec::new();
    for arg in &cmd.args {
        if arg.hide {
            continue;
        }
        if arg.long.as_deref() == Some("") || (arg.long.is_none() && arg.short.is_none()) {
            continue;
        }
        let mut names = Vec::new();
        if let Some(s) = arg.short {
            names.push(format!("-{s}"));
        }
        if let Some(ref l) = arg.long {
            if !l.is_empty() {
                names.push(format!("--{l}"));
            }
        }
        let help = arg.help.as_deref().unwrap_or("");
        opts.push((names.join(", "), help.to_string()));
    }

    if !opts.is_empty() {
        out.push_str("\nOptions:\n");
        for (names, help) in opts {
            out.push_str(&format!("  {names:<24} {help}\n"));
        }
    }

    out
}
