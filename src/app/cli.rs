//! The game's command line. `--game` is accepted and ignored: the launcher passes it.

/// What the command line asks for.
pub enum Command {
    /// The game, with these options.
    Play(LaunchOptions),
    /// `--connect <address>`: join, print what the server sent, exit.
    Connect(String),
    /// `--packets <file>`.
    Packets(String),
}

pub struct LaunchOptions {
    /// `--vulkan-validation`: the Vulkan validation layers.
    pub validation: bool,
    /// `--join <address>`: connecting instead of the title screen.
    pub join: Option<String>,
    /// `--bench`: the benchmark instead of playing.
    pub bench: Option<crate::game::bench::BenchOptions>,
}

/// `--instance <name>`: the instance to play in, if given.
pub fn instance(args: &[String]) -> Option<String> {
    let pos = args.iter().position(|a| a == "--instance")?;
    args.get(pos + 1).cloned()
}

/// Reads the command line. A flag missing its address is an error.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let value_of =
        |flag: &str| -> Result<Option<String>, String> {
            match args.iter().position(|a| a == flag) {
                None => Ok(None),
                Some(pos) => args.get(pos + 1).cloned().map(Some).ok_or_else(|| {
                    format!("{flag} braucht eine Adresse, z.B. {flag} 127.0.0.1:25565")
                }),
            }
        };
    if let Some(address) = value_of("--connect")? {
        return Ok(Command::Connect(address));
    }
    if let Some(file) = args.iter().position(|a| a == "--packets") {
        return args
            .get(file + 1)
            .cloned()
            .map(Command::Packets)
            .ok_or_else(|| "--packets braucht eine Datei".to_owned());
    }
    Ok(Command::Play(LaunchOptions {
        validation: args.iter().any(|a| a == "--vulkan-validation"),
        join: value_of("--join")?,
        bench: bench_options(args)?,
    }))
}

/// `--bench` and the flags that go with it.
fn bench_options(args: &[String]) -> Result<Option<crate::game::bench::BenchOptions>, String> {
    if !args.iter().any(|a| a == "--bench") {
        return Ok(None);
    }
    let value_of = |flag: &str| -> Result<Option<&String>, String> {
        match args.iter().position(|a| a == flag) {
            None => Ok(None),
            Some(pos) => args
                .get(pos + 1)
                .map(Some)
                .ok_or_else(|| format!("{flag} braucht einen Wert")),
        }
    };
    let fps_cap = match value_of("--fps-cap")? {
        None => None,
        Some(value) => Some(
            value
                .parse::<u32>()
                .ok()
                .filter(|&fps| fps > 0)
                .ok_or_else(|| format!("--fps-cap braucht eine Zahl, nicht {value}"))?,
        ),
    };
    Ok(Some(crate::game::bench::BenchOptions {
        label: value_of("--label")?
            .cloned()
            .unwrap_or_else(|| "bench".to_owned()),
        fps_cap,
        compare: value_of("--compare")?.map(std::path::PathBuf::from),
        scene: match value_of("--scene")? {
            None => crate::game::bench::Scene::Calm,
            Some(name) => crate::game::bench::Scene::parse(name)
                .ok_or_else(|| format!("--scene braucht calm oder active, nicht {name}"))?,
        },
        out: value_of("--out")?.map(std::path::PathBuf::from),
        hold: match value_of("--hold")? {
            None => None,
            Some(name) => Some(crate::game::bench::Phase::parse_holdable(name).ok_or_else(
                || format!("--hold braucht crowd, rise, turn, fly, sound oder idle, nicht {name}"),
            )?),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn reads_the_flags() {
        let Ok(Command::Play(options)) = parse(&args(&["melon", "--join", "a:1"])) else {
            panic!("expected Play");
        };
        assert_eq!(options.join.as_deref(), Some("a:1"));
        assert!(!options.validation);
        assert!(matches!(
            parse(&args(&["melon", "--connect", "b"])),
            Ok(Command::Connect(a)) if a == "b"
        ));
        assert!(parse(&args(&["melon", "--join"])).is_err());
    }

    #[test]
    fn reads_the_bench_flags() {
        let Ok(Command::Play(options)) = parse(&args(&[
            "melon",
            "--bench",
            "--label",
            "base",
            "--fps-cap",
            "144",
        ])) else {
            panic!("expected Play");
        };
        let bench = options.bench.expect("bench");
        assert_eq!(bench.label, "base");
        assert_eq!(bench.fps_cap, Some(144));
        assert!(bench.compare.is_none());
        assert!(parse(&args(&["melon", "--bench", "--fps-cap", "x"])).is_err());
        assert!(bench.hold.is_none());
        let Ok(Command::Play(held)) = parse(&args(&["melon", "--bench", "--hold", "turn"])) else {
            panic!("expected Play");
        };
        assert_eq!(
            held.bench.and_then(|b| b.hold),
            Some(crate::game::bench::Phase::Turn)
        );
        assert!(parse(&args(&["melon", "--bench", "--hold", "load"])).is_err());
        let Ok(Command::Play(plain)) = parse(&args(&["melon", "--game"])) else {
            panic!("expected Play");
        };
        assert!(plain.bench.is_none());
    }
}
