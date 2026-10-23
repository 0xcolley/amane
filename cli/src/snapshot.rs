use std::process::{Command, ExitCode, Stdio};

use crate::paths;

/*
 * a png of one window without showing it: the compiled shell runs once with
 * AMANE_SNAPSHOT set, draws the window offscreen and exits; the running
 * shell, if any, is left alone
 */
pub fn run(arguments: &[String]) -> ExitCode {
    if matches!(
        arguments.first().map(String::as_str),
        None | Some("-h" | "--help")
    ) {
        return usage(!arguments.is_empty());
    }

    let shell = paths::shell();

    if !shell.exists() {
        eprintln!("no compiled shell yet, run `amane compile` first");

        return ExitCode::FAILURE;
    }

    let request = match request(arguments) {
        Ok(request) => request,

        Err(error) => {
            eprintln!("failed to take a snapshot: {error}");

            return ExitCode::FAILURE;
        }
    };

    let status = Command::new(&shell)
        .env("AMANE_SNAPSHOT", request)
        .stdin(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,

        Ok(_) => ExitCode::FAILURE,

        Err(error) => {
            eprintln!("failed to run {}: {error}", shell.display());

            ExitCode::FAILURE
        }
    }
}

fn usage(asked: bool) -> ExitCode {
    eprintln!("usage: amane snapshot <window> [options]");
    eprintln!();
    eprintln!("  <window>              a layer window's namespace, or lock for the lock screen");
    eprintln!("  -o, --out <file>      where the png goes, <window>.png by default");
    eprintln!("  --scale <factor>      real pixels per logical pixel, 1 by default");
    eprintln!("  --size <width>x<height>");
    eprintln!("                        the monitor it's drawn for, 1440x960 by default");
    eprintln!("  --monitor <name>      the monitor's name, for views that tell monitors apart");
    eprintln!("  --ipc \"<name> [arguments]\"");
    eprintln!(
        "                        an ipc call made first, like --ipc \"control wifi\"; can repeat"
    );
    eprintln!("  --over <#rrggbb|picture>");
    eprintln!("                        drawn behind the window, blurred under its backdrops;");
    eprintln!("                        transparent by default");

    if asked {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// the arguments as the `key value` lines the shell reads
fn request(arguments: &[String]) -> Result<String, String> {
    let mut words = arguments.iter();

    let window = words.next().ok_or("name the window, or lock")?;

    if window.starts_with('-') {
        return Err(format!("name the window before {window}"));
    }

    let mut lines = vec![format!("window {window}")];
    let mut out = format!("{window}.png");

    while let Some(flag) = words.next() {
        let mut value = || {
            words
                .next()
                .filter(|value| !value.contains('\n'))
                .ok_or(format!("{flag} needs a value"))
        };

        match flag.as_str() {
            "-o" | "--out" => out = value()?.clone(),
            "--scale" => lines.push(format!("scale {}", value()?)),
            "--size" => lines.push(format!("size {}", value()?)),
            "--monitor" => lines.push(format!("monitor {}", value()?)),
            "--ipc" => lines.push(format!("call {}", value()?)),
            "--over" => lines.push(format!("over {}", value()?)),
            _ => return Err(format!("unknown option {flag}")),
        }
    }

    if out.contains('\n') {
        return Err(String::from("the file name can't contain a newline"));
    }

    lines.push(format!("out {out}"));

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(String::from).collect()
    }

    #[test]
    fn options_become_lines() {
        let request = request(&words("amane-control --ipc control --scale 2 -o cc.png"))
            .expect("failed to read the test arguments");

        assert_eq!(
            request,
            "window amane-control\ncall control\nscale 2\nout cc.png"
        );
    }

    #[test]
    fn the_window_is_named_first() {
        assert!(request(&[]).is_err());
        assert!(request(&words("--scale 2")).is_err());
        assert!(request(&words("bar --scale")).is_err());
        assert!(request(&words("bar --shade blue")).is_err());
    }
}
