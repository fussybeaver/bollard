mod buildkit;

use std::env;
use std::error::Error;

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    BuildkitUpdate,
    BuildkitCheck { online: bool },
}

fn main() -> Result<(), Box<dyn Error>> {
    let command = parse_command(env::args().skip(1))?;
    match command {
        Command::Help => {
            println!("{}", usage());
            Ok(())
        }
        Command::BuildkitUpdate => buildkit::update(),
        Command::BuildkitCheck { online } => buildkit::check(online),
    }
}

fn parse_command(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args: Vec<String> = args.into_iter().collect();
    match args.as_slice() {
        [buildkit, action] if buildkit == "buildkit" && action == "update" => {
            Ok(Command::BuildkitUpdate)
        }
        [buildkit, action] if buildkit == "buildkit" && action == "check" => {
            Ok(Command::BuildkitCheck { online: false })
        }
        [buildkit, action, online]
            if buildkit == "buildkit" && action == "check" && online == "--online" =>
        {
            Ok(Command::BuildkitCheck { online: true })
        }
        [flag] if flag == "--help" || flag == "-h" => Ok(Command::Help),
        _ => Err(usage().to_string()),
    }
}

fn usage() -> &'static str {
    "usage: cargo xtask buildkit <update|check [--online]>"
}

#[cfg(test)]
mod tests {
    use super::{parse_command, Command};

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    #[test]
    fn parses_supported_commands() {
        assert_eq!(
            parse_command(args(&["buildkit", "update"])),
            Ok(Command::BuildkitUpdate)
        );
        assert_eq!(
            parse_command(args(&["buildkit", "check"])),
            Ok(Command::BuildkitCheck { online: false })
        );
        assert_eq!(
            parse_command(args(&["buildkit", "check", "--online"])),
            Ok(Command::BuildkitCheck { online: true })
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(parse_command(args(&["buildkit", "fetch"])).is_err());
        assert!(parse_command(args(&["buildkit", "update", "--online"])).is_err());
        assert!(parse_command(args(&[])).is_err());
    }
}
