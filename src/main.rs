//! The game client. The launcher is the separate `melon-launcher` binary.

use melon::app::{self, cli};
use melon_assets::assets::Assets;
use melon_assets::i18n::languages::Language;
use melon_net;
use melon_settings::options::Config;

fn main() -> anyhow::Result<()> {
    melon_base::platform::drop_cargo_stack();
    // The frame is drawn here: ahead of the meshers and the server's generation.
    melon_base::platform::raise_frame_thread_priority();
    // The CPU's vector unit, probed once: every SIMD kernel runs on it.
    melon_core::math::simd::init();
    let args: Vec<String> = std::env::args().collect();
    let command = cli::parse(&args).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(1);
    });
    run_game(command)
}

fn run_game(command: cli::Command) -> anyhow::Result<()> {
    if let cli::Command::Packets(file) = &command {
        return network::play::debug::replay(std::path::Path::new(file));
    }
    let mut assets = Assets::discover()?;
    let args: Vec<String> = std::env::args().collect();
    let instance = cli::instance(&std::env::args().collect::<Vec<_>>())
        .unwrap_or_else(|| melon_core::instances::MELON.to_owned());
    assets = assets.with_instance(&instance)?;
    melon_core::instances::ensure_dirs(assets.game_dir());
    let _probe = start_probe(&args, &assets);

    let options = match command {
        cli::Command::Connect(address) => return connect(&address, &assets),
        cli::Command::Packets(_) => unreachable!("handled above"),
        cli::Command::Play(options) => options,
    };

    let mut config = Config::load(assets.game_dir());
    if let Some(language) = std::env::var("MELON_LANG")
        .ok()
        .and_then(|code| Language::from_code(&code))
    {
        config.lang = language;
    }
    app::run(options, assets, config)
}

/// `--connect`: joins a server and logs what it sent, no GUI.
fn connect(address: &str, assets: &Assets) -> anyhow::Result<()> {
    let identity = network::Identity::saved(assets.launcher_dir());
    let config = Config::load(assets.game_dir());
    let client = network::ClientInfo::from_config(&config);
    use melon_assets::assets::packs::server as packs;
    packs::begin(
        assets.game_dir(),
        None,
        packs::Policy::Enabled,
        packs::User {
            name: identity.username().to_owned(),
            uuid: identity.uuid(),
        },
    );
    let joining = {
        let address = address.to_owned();
        std::thread::spawn(move || network::join_server(&address, &identity, &client))
    };
    let answer_packs = || {
        while let Some(action) = packs::poll() {
            match action {
                packs::Action::Prompt(prompt) => {
                    println!(
                        "Server-Ressourcenpaket angefragt (Pflicht: {})",
                        prompt.required
                    );
                    packs::answer_prompt(true);
                }
                packs::Action::Reload => {
                    let loaded = packs::active_packs();
                    println!("Server-Ressourcenpakete geladen: {}", loaded.len());
                    packs::reload_finished(true);
                }
                packs::Action::Disconnect(reason) => println!("Trennen: {reason}"),
            }
        }
    };
    while !joining.is_finished() {
        answer_packs();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut session = match joining.join().expect("join thread") {
        Ok(session) => session,
        Err(e) => {
            eprintln!("Connection failed: {e:#}");
            std::process::exit(1);
        }
    };
    let identity = network::Identity::saved(assets.launcher_dir());
    println!(
        "Verbunden mit {address} als {}: {:#?}",
        identity.username(),
        session.info
    );
    let lang = melon_assets::i18n::Lang::load(assets, config.lang);
    let seconds = std::env::var("MELON_CONNECT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let mut command = std::env::var("MELON_CONNECT_COMMAND").ok();
    let command_at = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < until {
        answer_packs();
        for event in session.poll() {
            log_event(&event, &lang);
        }
        if std::time::Instant::now() >= command_at
            && let Some(command) = command.take()
        {
            println!("Sende /{command}");
            session.send_command(&command);
        }
        if let Some(reason) = session.ended() {
            println!("Verbindung beendet: {reason}");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

fn log_event(event: &network::WorldEvent, lang: &melon_assets::i18n::Lang) {
    use melon_net::play::game::GameEvent;
    let item = |slot: String, stack: &pumpkin_data::item_stack::ItemStack| {
        if stack.is_empty() {
            return;
        }
        let name = melon_gui::hud::item_slot::styled_hover_name(lang, stack);
        let components: Vec<String> = stack
            .patch
            .iter()
            .map(|(id, data)| match data {
                Some(_) => format!("{id:?}"),
                None => format!("-{id:?}"),
            })
            .collect();
        println!(
            "  {slot}: {} x{} \"{}\" [{}]",
            stack.item.registry_key,
            stack.item_count,
            name.to_plain(),
            components.join(", ")
        );
    };
    use melon_net::play::chat::ChatEvent;
    use melon_net::play::game::scores::ScorePacket;
    use melon_world::scoreboard::{ScoreUpdate, TeamAction};
    let text = |component: &melon_protocol::component::Component| {
        styled(&melon_gui::text::StyledText::from_component(
            component, lang,
        ))
    };
    match event {
        network::WorldEvent::Game(GameEvent::Chat(ChatEvent::ActionBar(component))) => {
            println!("Actionbar: {}", text(component));
        }
        network::WorldEvent::Game(GameEvent::Chat(ChatEvent::System {
            content,
            overlay: true,
        })) => println!("Actionbar: {}", text(content)),
        network::WorldEvent::Game(GameEvent::Scores(ScorePacket::TabList { header, footer })) => {
            println!("Tab-Kopf: {}\nTab-Fuss: {}", text(header), text(footer));
        }
        network::WorldEvent::Game(GameEvent::Scores(ScorePacket::Scoreboard(update))) => {
            match update {
                ScoreUpdate::SetObjective {
                    name, objective, ..
                } => println!("Objective {name}: {}", text(&objective.display_name)),
                ScoreUpdate::SetScore {
                    holder,
                    objective,
                    score,
                } => println!(
                    "Score {objective}/{holder} = {}{}",
                    score.value,
                    score
                        .display
                        .as_ref()
                        .map(|d| format!(" als {}", text(d)))
                        .unwrap_or_default()
                ),
                ScoreUpdate::Team {
                    name,
                    action: TeamAction::Add { look, .. } | TeamAction::Change(look),
                } => println!(
                    "Team {name}: Praefix {} Suffix {}",
                    text(&look.prefix),
                    text(&look.suffix)
                ),
                _ => {}
            }
        }
        network::WorldEvent::Reconfiguring => println!("Server: zurueck in die Konfiguration"),
        network::WorldEvent::Rejoined(info, events) => {
            println!("Neue Welt: {info:#?}");
            for event in events {
                log_event(event, lang);
            }
        }
        network::WorldEvent::Teleport(teleport) => println!("Spawn: {teleport:?}"),
        network::WorldEvent::ChunkLoaded(chunk) => println!("Chunk ({}, {})", chunk.x, chunk.z),
        network::WorldEvent::Game(GameEvent::ContainerContent {
            container, slots, ..
        }) => {
            println!("Inhalt von Container {container}:");
            for (i, stack) in slots.iter().enumerate() {
                item(format!("Slot {i}"), stack);
            }
        }
        network::WorldEvent::Game(GameEvent::ContainerSlot {
            container,
            slot,
            item: stack,
            ..
        }) => item(format!("Container {container} Slot {slot}"), stack),
        _ => {}
    }
}

/// A styled text as its runs.
fn styled(text: &melon_gui::text::StyledText) -> String {
    text.spans
        .iter()
        .map(|span| {
            let style = &span.style;
            let mut marks = Vec::new();
            if let Some([r, g, b]) = style.color {
                let byte = |c: f32| (c * 255.0).round() as u8;
                marks.push(format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b)));
            }
            for (on, mark) in [
                (style.bold, "fett"),
                (style.italic, "kursiv"),
                (style.underlined, "unterstrichen"),
                (style.strikethrough, "durchgestrichen"),
                (style.obfuscated, "verschleiert"),
            ] {
                if on {
                    marks.push(mark.to_owned());
                }
            }
            if let Some(font) = &style.font {
                marks.push(format!("Schrift {font}"));
            }
            format!("«{}»{{{}}}", span.text, marks.join(" "))
        })
        .collect()
}

/// Starts the tester when asked to; the report is written when the guard drops.
fn start_probe(args: &[String], assets: &Assets) -> Option<ProbeGuard> {
    if !args.iter().any(|a| a == "--probe") && std::env::var_os("MELON_PROBE").is_none() {
        return None;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = melon_core::instances::logs_dir(assets.game_dir()).join(format!("probe-{stamp}.md"));
    eprintln!("Tester on: report in {}", path.display());
    melon_core::probe::enable(path);
    Some(ProbeGuard)
}

struct ProbeGuard;

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        melon_core::probe::write_report();
        if let Some(path) = melon_core::probe::report_path() {
            eprintln!("Tester: report in {}", path.display());
        }
    }
}
