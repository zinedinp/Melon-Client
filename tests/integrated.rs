//! Against a real integrated server: each test opens a fresh world in a

use std::path::PathBuf;

use melon::network::{Identity, Session, Teleport, WorldEvent};
use melon::singleplayer::server::{self, IntegratedServer, Options};
use melon::singleplayer::{self, WorldSettings};
use melon::world::game_mode::GameMode;

/// A fresh world, its server, and a session joined to it. Dropping it
struct TestWorld {
    session: Session,
    server: Option<IntegratedServer>,
    dir: PathBuf,
    /// What arrived while joining: the first `poll`.
    joined: Vec<WorldEvent>,
    /// The first position sync among them.
    spawn: Teleport,
}

impl TestWorld {
    fn open(label: &str, settings: WorldSettings, view_distance: u8, username: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("melon-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (server, pipe) = server::start(Options {
            save_path: dir.clone(),
            game_dir: dir.join("game"),
            settings,
            view_distance,
            simulation_distance: view_distance,
        })
        .expect("the world should open");
        let identity = Identity::Offline {
            username: username.to_owned(),
        };
        let mut session =
            melon::network::join_stream(pipe, &identity, &Default::default(), Box::new(|| {}))
                .expect("joining the integrated server should work");
        let joined = session.poll();
        let spawn = joined
            .iter()
            .find_map(|e| match e {
                WorldEvent::Teleport(t) => Some(*t),
                _ => None,
            })
            .expect("the join waits for a spawn position");
        Self {
            session,
            server: Some(server),
            dir,
            joined,
            spawn,
        }
    }

    /// Leaves and stops the server, which saves the world.
    fn stop(&mut self) {
        self.session.close();
        self.server = None;
    }
}

impl Drop for TestWorld {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Opens a real world, joins it over the pipe, then saves and stops.
#[test]
#[ignore = "generates terrain and writes worlds to disk"]
fn joins_two_worlds_in_one_process() {
    for label in ["first", "second"] {
        let mut world = TestWorld::open(
            &format!("integrated-{label}"),
            WorldSettings::default(),
            2,
            "Tester",
        );
        let joined = std::mem::take(&mut world.joined);
        let mut chunks = joined
            .iter()
            .filter(|e| matches!(e, WorldEvent::ChunkLoaded(_)))
            .count();
        assert!(chunks > 0, "the spawn chunk should arrive");
        use melon::network::play::game::GameEvent;
        let mut health = None;
        let mut inventory = None;
        let note = |event: &GameEvent, health: &mut Option<f32>, inventory: &mut Option<usize>| {
            match event {
                GameEvent::Health { health: h, .. } => *health = Some(*h),
                GameEvent::ContainerContent {
                    container: 0,
                    slots,
                    ..
                } => *inventory = Some(slots.len()),
                _ => {}
            }
        };
        for event in &joined {
            if let WorldEvent::Game(event) = event {
                note(event, &mut health, &mut inventory);
            }
        }
        let Teleport {
            pos, yaw, pitch, ..
        } = world.spawn;
        let session = &mut world.session;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while (chunks < 25 || health.is_none() || inventory.is_none())
            && std::time::Instant::now() < until
        {
            for event in session.poll() {
                match event {
                    WorldEvent::ChunkLoaded(_) => chunks += 1,
                    WorldEvent::Game(event) => {
                        note(&event, &mut health, &mut inventory);
                    }
                    _ => {}
                }
            }
            assert!(session.ended().is_none(), "kicked: {:?}", session.ended());
            session.send_position(pos, yaw, pitch, true, false);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(chunks >= 25, "only {chunks} chunks arrived");
        assert_eq!(health, Some(20.0), "a new player has full health");
        assert_eq!(inventory, Some(melon::world::inventory::SLOT_COUNT));
        world.stop();
        assert!(
            world.dir.join("level.dat").is_file(),
            "level.dat should be saved"
        );
    }
}

/// Game modes and chat against the integrated server: the owner gets
#[test]
#[ignore = "starts an integrated server"]
fn game_mode_switching_and_chat_round_trip() {
    use melon::network::play::chat::ChatEvent;
    use melon::network::play::game::GameEvent;

    let settings = WorldSettings {
        name: "World".to_owned(),
        game_mode: singleplayer::WorldCreationMode::Survival,
        seed: "42".to_owned(),
        difficulty: singleplayer::Difficulty::Easy,
        allow_cheats: true,
        ..Default::default()
    };
    let mut world = TestWorld::open("chat", settings, 4, "Chatter");
    let mut events: Vec<GameEvent> = std::mem::take(&mut world.joined)
        .into_iter()
        .filter_map(|e| match e {
            WorldEvent::Game(event) => Some(event),
            _ => None,
        })
        .collect();
    let session = &mut world.session;
    let entity_id = session.info.entity_id;
    let wait_for = |session: &mut melon::network::Session,
                    events: &mut Vec<GameEvent>,
                    what: &str,
                    found: &dyn Fn(&GameEvent) -> bool| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if let Some(i) = events.iter().position(found) {
                return events.remove(i);
            }
            assert!(std::time::Instant::now() < until, "no {what} arrived");
            for event in session.poll() {
                if let WorldEvent::Game(event) = event {
                    events.push(event);
                }
            }
            assert!(session.ended().is_none(), "{:?}", session.ended());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };

    let level = wait_for(
        session,
        &mut events,
        "permission level",
        &|e| matches!(e, GameEvent::PermissionLevel { entity_id: id, .. } if *id == entity_id),
    );
    let GameEvent::PermissionLevel { level, .. } = level else {
        unreachable!()
    };
    assert_eq!(level, 4);

    session.send_change_game_mode(GameMode::Creative);
    wait_for(session, &mut events, "game mode change", &|e| {
        matches!(e, GameEvent::GameMode(GameMode::Creative))
    });
    let abilities = wait_for(
        session,
        &mut events,
        "abilities",
        &|e| matches!(e, GameEvent::Abilities { flags, .. } if flags & 0x04 != 0),
    );
    assert!(matches!(abilities, GameEvent::Abilities { .. }));

    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    let book = melon::world::creative_tabs::TABS
        .iter()
        .flat_map(|t| &t.items)
        .find(|s| s.item.registry_key == "enchanted_book")
        .cloned()
        .expect("an enchanted book");
    session.send_creative_slot(36, &ItemStack::new(64, &Item::DIAMOND));
    session.send_creative_slot(37, &book);
    session.send_creative_slot(-1, &ItemStack::new(1, &Item::STONE));

    session.send_chat("hallo welt", 0);
    let chat = wait_for(session, &mut events, "the chat message", &|e| {
        matches!(e, GameEvent::Chat(ChatEvent::Player { message, .. }) if message == "hallo welt")
            || matches!(e, GameEvent::Chat(ChatEvent::Disguised { .. }))
            || matches!(e, GameEvent::Chat(ChatEvent::System { content, .. })
                if format!("{content:?}").contains("hallo welt"))
    });
    if let GameEvent::Chat(ChatEvent::Disguised { bound, .. }) = &chat {
        let decoration = session.info.chat_types.get(bound.chat_type);
        println!("chat type {}: {decoration:?}", bound.chat_type);
        assert_eq!(decoration.parameters, vec!["content".to_owned()]);
    }

    let tree = wait_for(session, &mut events, "the command tree", &|e| {
        matches!(e, GameEvent::Commands(_))
    });
    let GameEvent::Commands(tree) = tree else {
        unreachable!()
    };
    let gamem = tree.complete("gamem");
    assert_eq!(
        (gamem.start, gamem.literals),
        (0, vec!["gamemode".to_owned()])
    );

    let source = melon::network::play::chat::command_suggestions::Source {
        registries: &session.info.registries,
        players: &[],
        selectors: true,
    };
    let enchant = tree.complete("enchant @a eff");
    let found: Vec<String> = enchant
        .arguments
        .iter()
        .filter_map(|a| melon::network::play::chat::command_suggestions::suggest(a, "eff", &source))
        .flatten()
        .map(|s| s.text)
        .collect();
    assert_eq!(found, ["minecraft:efficiency"]);

    session.send_command_suggestion(7, "/give Ch");
    let suggestions = wait_for(
        session,
        &mut events,
        "suggestions",
        &|e| matches!(e, GameEvent::Chat(ChatEvent::Suggestions(s)) if s.transaction == 7),
    );
    let GameEvent::Chat(ChatEvent::Suggestions(suggestions)) = suggestions else {
        unreachable!()
    };
    println!("suggestions: {suggestions:?}");
    assert!(!suggestions.matches.is_empty());

    session.send_command("gamemode survival");
    wait_for(session, &mut events, "survival again", &|e| {
        matches!(e, GameEvent::GameMode(GameMode::Survival))
    });
}

/// Cave culling on real terrain: deep in the stone the walk reaches only
#[test]
#[ignore = "starts an integrated server"]
fn cave_culling_hides_terrain_from_underground() {
    use melon::world::culling::occlusion::{self, SectionState};
    use melon::world::terrain::section_visibility;
    use std::collections::HashMap;

    let settings = WorldSettings {
        seed: "4831".to_owned(),
        ..WorldSettings::default()
    };
    let mut world = TestWorld::open("culling", settings, 4, "Caver");
    let mut level = melon::world::level::Level::default();
    let mut visibility = HashMap::new();
    let mut insert = |level: &mut melon::world::level::Level, chunk: melon::world::chunk::Chunk| {
        visibility.insert((chunk.x, chunk.z), section_visibility(&chunk));
        level.insert(chunk);
    };
    let session = &mut world.session;
    for event in std::mem::take(&mut world.joined) {
        if let WorldEvent::ChunkLoaded(chunk) = event {
            insert(&mut level, chunk);
        }
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while level.chunk_count() < 81 && std::time::Instant::now() < until {
        for event in session.poll() {
            if let WorldEvent::ChunkLoaded(chunk) = event {
                insert(&mut level, chunk);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (x, y, z) = world.spawn.pos;
    let (cx, cz) = (
        melon::math::section_coord(x as i32),
        melon::math::section_coord(z as i32),
    );
    let (min_y, height) = level.height();
    let section_y = |block_y: i32| melon::math::section_coord(block_y - min_y);
    let reached = |block_y: i32| {
        occlusion::walk(
            (cx, section_y(block_y), cz),
            height,
            4,
            None,
            |(x, y, z)| match visibility.get(&(x, z)) {
                Some(sections) => SectionState::Loaded(sections[y as usize]),
                None => SectionState::Missing,
            },
        )
        .count()
    };
    let surface = reached(y as i32 + 2);
    let deep = reached(-40);
    println!("sections reached: surface {surface}, deep {deep}");
    assert!(
        deep * 4 < surface,
        "underground reached {deep} sections, the surface {surface}"
    );
}

/// Walks around a freshly generated world with the real physics: the
#[test]
#[ignore = "generates terrain and writes worlds to disk"]
fn the_player_stands_and_walks_on_generated_terrain() {
    use glam::DVec3;
    use melon::world::player::{MoveInput, PlayerState};

    let settings = WorldSettings {
        seed: "4831".to_owned(),
        ..WorldSettings::default()
    };
    let mut world = TestWorld::open("physics", settings, 2, "Walker");
    let mut level = melon::world::level::Level::default();
    for event in std::mem::take(&mut world.joined) {
        if let WorldEvent::ChunkLoaded(chunk) = event {
            level.insert(chunk);
        }
    }
    let (x, y, z) = world.spawn.pos;
    let session = &mut world.session;
    let mut player = PlayerState::new(DVec3::new(x, y, z));

    let mut pump = |level: &mut melon::world::level::Level, player: &mut PlayerState| {
        for event in session.poll() {
            match event {
                WorldEvent::ChunkLoaded(chunk) => level.insert(chunk),
                WorldEvent::Teleport(t) => {
                    let feet = DVec3::new(t.pos.0, t.pos.1, t.pos.2);
                    player.teleport(feet, DVec3::ZERO, feet);
                }
                _ => {}
            }
        }
        let feet = player.feet();
        session.send_position(
            (feet.x, feet.y, feet.z),
            0.0,
            0.0,
            player.on_ground(),
            false,
        );
    };
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut settled_ticks = 0;
    while settled_ticks < 100 && std::time::Instant::now() < until {
        pump(&mut level, &mut player);
        player.tick(
            MoveInput::default(),
            &level,
            20,
            melon::world::player::SPRINT_WINDOW_DEFAULT,
        );
        if player.on_ground() {
            settled_ticks += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        player.on_ground(),
        "never landed, feet at {:?}",
        player.feet()
    );
    let rest = player.feet();
    assert!(
        (rest.y - y).abs() < 3.0,
        "landed at {rest:?}, far from the spawn at y {y}"
    );

    // The server's move checks must not mistake walking for cheating.
    let mut snapped_back = 0;
    for tick in 0..60 {
        for event in session.poll() {
            match event {
                WorldEvent::ChunkLoaded(chunk) => level.insert(chunk),
                WorldEvent::Teleport(t) => {
                    snapped_back += 1;
                    let feet = DVec3::new(t.pos.0, t.pos.1, t.pos.2);
                    player.teleport(feet, DVec3::ZERO, feet);
                }
                _ => {}
            }
        }
        let feet = player.feet();
        session.send_position(
            (feet.x, feet.y, feet.z),
            0.0,
            0.0,
            player.on_ground(),
            false,
        );
        let input = MoveInput {
            forward: 1.0,
            jump: tick % 20 == 0,
            ..MoveInput::default()
        };
        player.tick(
            input,
            &level,
            20,
            melon::world::player::SPRINT_WINDOW_DEFAULT,
        );
        let feet = player.feet() + DVec3::new(0.0, 0.01, 0.0);
        let inside = melon::world::physics::block_boxes(
            &level,
            &melon::world::physics::Aabb::at_feet(feet, 0.5, 1.7),
        );
        assert!(inside.is_empty(), "inside a block at {feet:?}: {inside:?}");
    }
    assert!(
        (player.feet() - rest).length() > 1.0,
        "walking went nowhere from {rest:?}"
    );
    assert_eq!(snapped_back, 0, "the server rejected a walking move");
}

/// `melon:movement`: the server lists what it allows, its "moved too
/// quickly" check snaps a jump back, and a started state's grace lets the
/// same jump through.
#[test]
#[ignore = "starts an integrated server"]
fn a_custom_movement_state_bends_the_move_checks() {
    use pumpkin_data::mod_movement::{MovementSpec, register_movement};
    use pumpkin_protocol::java::melon_movement::{CHANNEL, MovementPayload};

    register_movement(MovementSpec {
        id: "itest:blink".to_owned(),
        extra_speed: 20.0,
        ..MovementSpec::default()
    })
    .unwrap();
    let mut world = TestWorld::open("movement", WorldSettings::default(), 2, "Blinker");
    let (x, y, z) = world.spawn.pos;
    let session = &mut world.session;
    session.send_register(&[CHANNEL]);

    // Polls for up to `secs`; the first teleport (acked), or `None`.
    let wait = |session: &mut Session, secs: u64, allowed: &mut Option<Vec<String>>| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        while std::time::Instant::now() < until {
            for event in session.poll() {
                match event {
                    WorldEvent::Movement(MovementPayload::Allowed(ids)) => *allowed = Some(ids),
                    WorldEvent::Teleport(t) => {
                        session.send_accept_teleport(t.id, t.pos, t.yaw, t.pitch);
                        return Some(t);
                    }
                    _ => {}
                }
            }
            assert!(session.ended().is_none(), "{:?}", session.ended());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        None
    };
    let mut allowed = None;
    for _ in 0..10 {
        wait(session, 1, &mut allowed);
        if allowed.is_some() {
            break;
        }
    }
    assert!(
        allowed
            .expect("no Allowed arrived")
            .iter()
            .any(|id| id == "itest:blink"),
        "the server lists the registered state"
    );

    let up = (x, y + 15.0, z);
    session.send_position(up, 0.0, 0.0, false, false);
    let back = wait(session, 3, &mut None).expect("a 15 block jump snaps back");
    assert!((back.pos.1 - y).abs() < 1.0, "{back:?}");

    let start = MovementPayload::Start {
        id: "itest:blink".to_owned(),
    };
    session.send_custom_payload(CHANNEL, &start.encode());
    session.send_position(up, 0.0, 0.0, false, false);
    assert!(
        wait(session, 2, &mut None).is_none(),
        "the started state's grace lets the jump through"
    );
}

/// Writes the command tree an operator gets, in the shape of vanilla's
#[test]
#[ignore = "starts an integrated server; set MELON_COMMANDS_DUMP"]
fn dump_command_tree() {
    use melon::network::play::chat::commands::{CommandTree, NodeKind};
    use melon::network::play::game::GameEvent;
    use serde_json::{Map, Value, json};

    let Ok(path) = std::env::var("MELON_COMMANDS_DUMP") else {
        return;
    };
    let settings = WorldSettings {
        name: "World".to_owned(),
        game_mode: singleplayer::WorldCreationMode::Creative,
        seed: "42".to_owned(),
        difficulty: singleplayer::Difficulty::Easy,
        allow_cheats: true,
        ..Default::default()
    };
    let mut world = TestWorld::open("commands", settings, 2, "Dumper");
    let mut tree = None;
    let mut take = |events: Vec<WorldEvent>| {
        for event in events {
            if let WorldEvent::Game(GameEvent::Commands(t)) = event {
                tree = Some(t);
            }
        }
    };
    take(std::mem::take(&mut world.joined));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < until {
        take(world.session.poll());
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let tree = tree.expect("a command tree");

    const PARSERS: &[&str] = &[
        "brigadier:bool",
        "brigadier:float",
        "brigadier:double",
        "brigadier:integer",
        "brigadier:long",
        "brigadier:string",
        "entity",
        "game_profile",
        "block_pos",
        "column_pos",
        "vec3",
        "vec2",
        "block_state",
        "block_predicate",
        "item_stack",
        "item_predicate",
        "team_color",
        "hex_color",
        "component",
        "style",
        "message",
        "nbt_compound_tag",
        "nbt_tag",
        "nbt_path",
        "objective",
        "objective_criteria",
        "operation",
        "particle",
        "angle",
        "rotation",
        "scoreboard_slot",
        "score_holder",
        "swizzle",
        "team",
        "item_slot",
        "item_slots",
        "resource_location",
        "function",
        "entity_anchor",
        "int_range",
        "float_range",
        "dimension",
        "gamemode",
        "time",
        "resource_or_tag",
        "resource_or_tag_key",
        "resource",
        "resource_key",
        "resource_selector",
        "template_mirror",
        "template_rotation",
        "heightmap",
        "loot_table",
        "loot_predicate",
        "loot_modifier",
        "context_float_provider",
        "context_int_provider",
        "slot_source",
        "dialog",
        "feature",
        "swing_animation",
        "uuid",
    ];
    fn paths(tree: &CommandTree) -> Vec<Option<Vec<String>>> {
        let mut out = vec![None; tree.nodes.len()];
        out[tree.root] = Some(Vec::new());
        let mut queue = std::collections::VecDeque::from([tree.root]);
        while let Some(n) = queue.pop_front() {
            for &c in &tree.nodes[n].children {
                if out[c].is_none() {
                    let mut path = out[n].clone().unwrap();
                    path.push(match &tree.nodes[c].kind {
                        NodeKind::Literal(name) => name.to_string(),
                        NodeKind::Argument(a) => a.name.to_string(),
                        NodeKind::Root => String::new(),
                    });
                    out[c] = Some(path);
                    queue.push_back(c);
                }
            }
        }
        out
    }
    fn node(tree: &CommandTree, paths: &[Option<Vec<String>>], n: usize, depth: usize) -> Value {
        let node = &tree.nodes[n];
        let mut out = Map::new();
        match &node.kind {
            NodeKind::Root => out.insert("type".into(), json!("root")),
            NodeKind::Literal(_) => out.insert("type".into(), json!("literal")),
            NodeKind::Argument(a) => {
                out.insert("type".into(), json!("argument"));
                let name = usize::try_from(a.parser_id)
                    .ok()
                    .and_then(|i| PARSERS.get(i))
                    .map_or_else(
                        || a.parser_id.to_string(),
                        |p| {
                            if p.contains(':') {
                                (*p).to_owned()
                            } else {
                                format!("minecraft:{p}")
                            }
                        },
                    );
                out.insert("parser".into(), json!(name));
                if let melon::network::play::chat::commands::Parser::Registry { registry, .. } =
                    &a.parser
                {
                    out.insert("registry".into(), json!(registry));
                }
                out.insert("suggestions".into(), json!(a.suggestions));
                out.insert("greedy".into(), json!(a.greedy))
            }
        };
        if node.executable {
            out.insert("executable".into(), json!(true));
        }
        if let Some(target) = node.redirect {
            out.insert("redirect".into(), json!(paths[target]));
        }
        if !node.children.is_empty() && depth < 64 {
            let children: Map<String, Value> = node
                .children
                .iter()
                .map(|&c| {
                    let name = match &tree.nodes[c].kind {
                        NodeKind::Literal(name) => name.to_string(),
                        NodeKind::Argument(a) => a.name.to_string(),
                        NodeKind::Root => String::new(),
                    };
                    (name, node_value(tree, paths, c, depth))
                })
                .collect();
            out.insert("children".into(), Value::Object(children));
        }
        Value::Object(out)
    }
    fn node_value(
        tree: &CommandTree,
        paths: &[Option<Vec<String>>],
        n: usize,
        depth: usize,
    ) -> Value {
        node(tree, paths, n, depth + 1)
    }
    let paths = paths(&tree);
    let dump = node(&tree, &paths, tree.root, 0);
    std::fs::write(&path, serde_json::to_string_pretty(&dump).unwrap()).unwrap();
    println!("wrote {path}");
}

/// Another dimension on a real server: a command sends the player to the
#[test]
#[ignore = "starts an integrated server"]
fn the_nether_comes_with_its_own_height() {
    use melon::network::play::game::GameEvent;
    use melon::world::dimension::Height;

    let settings = WorldSettings {
        seed: "42".to_owned(),
        allow_cheats: true,
        ..WorldSettings::default()
    };
    let mut world = TestWorld::open("nether", settings, 2, "Traveller");
    assert_eq!(
        world.session.info.dimension_types.get(0).height,
        Height::OVERWORLD
    );
    assert!(
        !world.session.info.biomes.is_empty(),
        "the biome registry arrives in the configuration phase"
    );
    let overworld = world.session.info.dimension_name.clone();
    assert_eq!(overworld, "minecraft:overworld");
    let Teleport {
        pos, yaw, pitch, ..
    } = world.spawn;

    let mut level = melon::world::level::Level::default();
    let mut op = false;
    for event in std::mem::take(&mut world.joined) {
        match event {
            WorldEvent::ChunkLoaded(chunk) => {
                assert_eq!(chunk.height, Height::OVERWORLD);
                level.insert(chunk);
            }
            WorldEvent::Game(GameEvent::PermissionLevel { level: 4, .. }) => op = true,
            _ => {}
        }
    }
    let session = &mut world.session;
    let started = std::time::Instant::now();
    let until = started + std::time::Duration::from_secs(90);
    let mut sent = false;
    let mut in_nether = false;
    let mut nether_chunks = 0;
    let mut teleported = false;
    while std::time::Instant::now() < until && (nether_chunks < 9 || !teleported) {
        if op && !sent {
            println!("asking for the nether");
            session.send_command("execute in minecraft:the_nether run tp @s 0 100 0");
            sent = true;
        }
        for event in session.poll() {
            match event {
                WorldEvent::Game(GameEvent::PermissionLevel { level: 4, .. }) => op = true,
                WorldEvent::Game(GameEvent::Respawn {
                    dimension_id,
                    dimension_name,
                    ..
                }) => {
                    println!("respawn into {dimension_name} ({dimension_id})");
                    assert_eq!(dimension_name, "minecraft:the_nether");
                    let dimension = session.info.dimension_types.get(dimension_id);
                    assert_eq!(dimension.height, Height::NETHER);
                    assert!(!dimension.has_skylight);
                    level.set_dimension(&dimension);
                    in_nether = true;
                }
                WorldEvent::ChunkLoaded(chunk) if in_nether => {
                    assert_eq!(
                        chunk.height,
                        Height::NETHER,
                        "decoded by the nether's height"
                    );
                    assert_eq!(chunk.sections.len(), 16);
                    assert_eq!(chunk.biomes.sections.len(), 16);
                    level.insert(chunk);
                    assert_eq!(level.light_at(0, 100, 0).1, 0, "no sky light in the nether");
                    nether_chunks += 1;
                    println!(
                        "  nether chunk {nether_chunks} after {:.1}s",
                        started.elapsed().as_secs_f32()
                    );
                }
                WorldEvent::Teleport(t) if in_nether => {
                    assert_eq!((t.pos.0, t.pos.1, t.pos.2), (0.5, 100.0, 0.5));
                    println!("  teleport after {:.1}s", started.elapsed().as_secs_f32());
                    teleported = true;
                }
                other if in_nether => {
                    let kind = match &other {
                        WorldEvent::ChunkLoaded(_) => "chunk",
                        WorldEvent::ChunkUnloaded(..) => "unload",
                        WorldEvent::LightUpdated { .. } => "light",
                        WorldEvent::BiomesChanged(_) => "biomes",
                        WorldEvent::Teleport(_) => "teleport",
                        WorldEvent::Ping(_) => "ping",
                        WorldEvent::Movement(_) => "movement",
                        WorldEvent::ServerChannels(_) => "channels",
                        WorldEvent::Game(_) => "game",
                        WorldEvent::Reconfiguring => "reconfiguring",
                        WorldEvent::Rejoined(..) => "rejoined",
                    };
                    println!("  {kind} after {:.1}s", started.elapsed().as_secs_f32());
                }
                _ => {}
            }
        }
        assert!(session.ended().is_none(), "kicked: {:?}", session.ended());
        if !in_nether {
            session.send_position(pos, yaw, pitch, true, false);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!(
        "loop over: sent {sent}, nether {in_nether}, chunks {nether_chunks}, teleported {teleported}"
    );
    assert!(sent, "the operator level never arrived");
    assert!(in_nether, "the server never sent us to the nether");
    assert!(
        nether_chunks >= 9,
        "only {nether_chunks} nether chunks decoded"
    );
    assert!(teleported, "no position in the nether");
    assert_eq!(level.world_height(), Height::NETHER);
    assert!(!level.has_skylight());
    assert!(level.chunk_count() >= 9);
    let biome = level.biome_at(0, 100, 0).expect("the chunk at the origin");
    let name = session.info.biomes.name(biome).unwrap_or("?").to_owned();
    println!("biome at the nether origin: {name}");
    assert!(
        [
            "nether_wastes",
            "soul_sand_valley",
            "crimson_forest",
            "warped_forest",
            "basalt_deltas"
        ]
        .iter()
        .any(|n| name.ends_with(n)),
        "{name}"
    );
}

/// Summoned mobs reach the client: each starter kind is tracked with its
#[test]
#[ignore = "starts an integrated server"]
fn summoned_mobs_are_tracked() {
    use melon::network::play::game::GameEvent;
    use melon::world::entity::kind::EntityKind;
    use melon::world::entity::tracker::Entities;

    let settings = WorldSettings {
        name: "World".to_owned(),
        game_mode: singleplayer::WorldCreationMode::Creative,
        seed: "42".to_owned(),
        difficulty: singleplayer::Difficulty::Easy,
        allow_cheats: true,
        ..Default::default()
    };
    let mut world = TestWorld::open("mobs", settings, 4, "Herder");
    let own = world.session.info.entity_id;
    let mut entities = Entities::default();
    let feed = |events: Vec<WorldEvent>, entities: &mut Entities| {
        for event in events {
            if let WorldEvent::Game(GameEvent::Entity(packet)) = event {
                entities.apply(packet, own);
            }
        }
    };
    feed(std::mem::take(&mut world.joined), &mut entities);
    let session = &mut world.session;
    let wait = |session: &mut Session,
                entities: &mut Entities,
                what: &str,
                done: &dyn Fn(&Entities) -> bool| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !done(entities) {
            assert!(std::time::Instant::now() < until, "{what} didn't happen");
            feed(session.poll(), entities);
            entities.tick(true);
            assert!(session.ended().is_none(), "{:?}", session.ended());
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    std::thread::sleep(std::time::Duration::from_secs(1));
    feed(session.poll(), &mut entities);

    let kinds = [
        ("pig", EntityKind::Pig),
        ("cow", EntityKind::Cow),
        ("sheep", EntityKind::Sheep),
        ("chicken", EntityKind::Chicken),
        ("creeper", EntityKind::Creeper),
        ("zombie", EntityKind::Zombie),
        ("skeleton", EntityKind::Skeleton),
        ("spider", EntityKind::Spider),
    ];
    for (i, (name, _)) in kinds.iter().enumerate() {
        session.send_command(&format!(
            "summon minecraft:{name} ~{} ~ ~3",
            i as i32 * 2 - 8
        ));
    }
    wait(session, &mut entities, "all mobs tracked", &|e| {
        kinds
            .iter()
            .all(|(_, kind)| e.iter().any(|entity| entity.kind == *kind))
    });
    let spawn = world.spawn.pos;
    for (name, kind) in kinds {
        let near = entities.iter().filter(|e| e.kind == kind).any(|e| {
            let at = e.pos(1.0);
            (at.x - spawn.0).abs() < 16.0
                && (at.z - spawn.2).abs() < 16.0
                && (at.y - spawn.1).abs() < 4.0
        });
        assert!(near, "no {name} where it was summoned");
    }

    session.send_command("summon minecraft:zombie ~ ~ ~-3 {IsBaby:1b}");
    let babies = |e: &Entities| {
        e.iter()
            .filter(|z| z.kind == EntityKind::Zombie && z.appearance.baby)
            .count()
    };
    wait(session, &mut entities, "a baby zombie", &|e| babies(e) > 0);

    session.send_command("kill @e[type=minecraft:pig]");
    wait(session, &mut entities, "the pig removed", &|e| {
        !e.iter().any(|entity| entity.kind == EntityKind::Pig)
    });
}

/// A creative world with entities tracked as the game does, for tests
struct MobWorld {
    world: TestWorld,
    entities: melon::world::entity::tracker::Entities,
    /// Every `ClientboundMoveVehicle`: where the server put the vehicle back.
    vehicle_resets: Vec<[f64; 3]>,
}

impl MobWorld {
    fn open(label: &str, username: &str) -> Self {
        let settings = WorldSettings {
            name: "World".to_owned(),
            game_mode: singleplayer::WorldCreationMode::Creative,
            seed: "42".to_owned(),
            difficulty: singleplayer::Difficulty::Easy,
            allow_cheats: true,
            ..Default::default()
        };
        let mut world = TestWorld::open(label, settings, 4, username);
        let joined = std::mem::take(&mut world.joined);
        let mut this = Self {
            world,
            entities: Default::default(),
            vehicle_resets: Vec::new(),
        };
        this.feed(joined);
        std::thread::sleep(std::time::Duration::from_secs(1));
        this.wait("nothing", |_| true);
        this
    }

    fn feed(&mut self, events: Vec<WorldEvent>) {
        use melon::network::play::game::GameEvent;
        let own = self.world.session.info.entity_id;
        for event in events {
            match event {
                WorldEvent::Game(GameEvent::Entity(packet)) => self.entities.apply(packet, own),
                WorldEvent::Game(GameEvent::MoveVehicle { pos, .. }) => {
                    self.vehicle_resets.push(pos);
                }
                _ => {}
            }
        }
    }

    /// Polls and ticks until `done`, for at most 30 s.
    fn wait(
        &mut self,
        what: &str,
        done: impl Fn(&melon::world::entity::tracker::Entities) -> bool,
    ) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let events = self.world.session.poll();
            self.feed(events);
            self.entities.tick(true);
            assert!(
                self.world.session.ended().is_none(),
                "{:?}",
                self.world.session.ended()
            );
            if done(&self.entities) {
                return;
            }
            assert!(std::time::Instant::now() < until, "{what} didn't happen");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Summons a still `name` two blocks south and waits for it. Others
    fn summon(&mut self, name: &str, kind: melon::world::entity::kind::EntityKind) -> i32 {
        let before: std::collections::HashSet<i32> = self.entities.iter().map(|e| e.id).collect();
        self.world
            .session
            .send_command(&format!("summon minecraft:{name} ~ ~ ~2 {{NoAI:1b}}"));
        let find = |e: &melon::world::entity::tracker::Entities| {
            e.iter()
                .find(|m| m.kind == kind && !before.contains(&m.id))
                .map(|m| m.id)
        };
        self.wait(&format!("the {name} tracked"), |e| find(e).is_some());
        find(&self.entities).unwrap()
    }
}

/// An attack packet reaches the mob: a pig two blocks off, hit with a
#[test]
#[ignore = "starts an integrated server"]
fn an_attacked_pig_is_hurt() {
    use melon::world::entity::kind::EntityKind;

    let mut mobs = MobWorld::open("attack", "Hunter");
    let id = mobs.summon("pig", EntityKind::Pig);
    mobs.world.session.send_attack(id);
    mobs.wait("the pig hurt", |e| {
        e.get(id).is_some_and(|p| p.living_state(1.0).hurt)
    });
}

/// An interact packet reaches the mob: flint and steel on a creeper
#[test]
#[ignore = "starts an integrated server"]
fn flint_and_steel_lights_a_creeper() {
    use melon::world::entity::kind::EntityKind;
    use melon::world::player::hands::Hand;

    let mut mobs = MobWorld::open("interact", "Igniter");
    mobs.world
        .session
        .send_command("give @s minecraft:flint_and_steel");
    let id = mobs.summon("creeper", EntityKind::Creeper);
    assert!(!mobs.entities.get(id).unwrap().appearance.ignited);
    mobs.world
        .session
        .send_interact(id, Hand::Main, [0.0, 1.0, -0.3], false);
    mobs.wait("the creeper lit", |e| {
        e.get(id).is_some_and(|c| c.appearance.ignited)
    });
}

/// Leads reach the tracker: a lead used on a pig ties it to the player,
#[test]
#[ignore = "starts an integrated server"]
fn a_lead_ties_a_pig_to_the_player_and_lets_go() {
    use melon::world::entity::kind::EntityKind;
    use melon::world::player::hands::Hand;

    let mut mobs = MobWorld::open("lead", "Shepherd");
    let own = mobs.world.session.info.entity_id;
    mobs.world.session.send_command("give @s minecraft:lead");
    let id = mobs.summon("pig", EntityKind::Pig);
    assert_eq!(mobs.entities.get(id).unwrap().leash_holder, None);
    mobs.world
        .session
        .send_interact(id, Hand::Main, [0.0, 0.5, 0.0], false);
    mobs.wait("the pig tied", |e| {
        e.get(id).is_some_and(|p| p.leash_holder == Some(own))
    });
    mobs.world
        .session
        .send_interact(id, Hand::Main, [0.0, 0.5, 0.0], false);
    mobs.wait("the pig let go", |e| {
        e.get(id).is_some_and(|p| p.leash_holder != Some(own))
    });
}

/// Riding reaches the tracker: a click boards a boat and a minecart, the
#[test]
#[ignore = "starts an integrated server"]
fn a_click_boards_a_boat_and_sneaking_gets_off() {
    use melon::network::play::game::PlayerInput;
    use melon::world::entity::kind::EntityKind;
    use melon::world::entity::ride::Steering;
    use melon::world::player::hands::Hand;
    use pumpkin_data::item_stack::ItemStack;

    let mut mobs = MobWorld::open("ride", "Skipper");
    for (name, kind) in [
        ("oak_boat", EntityKind::Boat),
        ("minecart", EntityKind::Minecart),
    ] {
        let id = mobs.summon(name, kind);
        assert_eq!(mobs.entities.own_vehicle(), None);
        mobs.world
            .session
            .send_interact(id, Hand::Main, [0.0, 0.2, 0.0], false);
        mobs.wait(&format!("the {name} boarded"), |e| {
            e.own_vehicle() == Some(id)
        });
        let vehicle = mobs.entities.get(id).unwrap();
        assert_eq!(vehicle.passengers.len(), 1);
        let seat = mobs.entities.own_seat().unwrap();
        let at = vehicle.tick_pos();
        assert!((seat.x - at.x).abs() < 1e-6 && (seat.z - at.z).abs() < 1e-6);
        assert!(seat.y > at.y - 0.6 && seat.y < at.y + 1.0, "{seat} {at}");
        let steering = mobs
            .entities
            .own_steering(ItemStack::EMPTY, ItemStack::EMPTY)
            .unwrap();
        assert_eq!(
            steering,
            if kind == EntityKind::Boat {
                Steering::Paddles
            } else {
                Steering::Carried
            }
        );
        mobs.world.session.send_player_input(PlayerInput {
            sneak: true,
            ..Default::default()
        });
        mobs.wait(&format!("off the {name}"), |e| e.own_vehicle().is_none());
        mobs.world.session.send_player_input(PlayerInput::default());
        mobs.world
            .session
            .send_command(&format!("kill @e[type=minecraft:{name}]"));
        mobs.wait(&format!("the {name} gone"), |e| e.get(id).is_none());
    }
}

/// The boat the player steers goes where its client says, within what
/// the server's checks allow: a short paddle is taken and seen by the
/// boat's watchers, a jump far off is put back with
/// `ClientboundMoveVehicle`, and the paddles rest once nobody rows.
#[test]
#[ignore = "starts an integrated server"]
fn a_steered_boat_follows_its_client_until_it_jumps() {
    use melon::network::play::game::PlayerInput;
    use melon::world::entity::kind::EntityKind;
    use melon::world::player::hands::Hand;

    let mut mobs = MobWorld::open("boat", "Rower");
    let id = mobs.summon("oak_boat", EntityKind::Boat);
    mobs.world
        .session
        .send_interact(id, Hand::Main, [0.0, 0.2, 0.0], false);
    mobs.wait("the boat boarded", |e| e.own_vehicle() == Some(id));
    // The server takes the boat's moves from the tick after boarding.
    std::thread::sleep(std::time::Duration::from_millis(200));
    let at = mobs.entities.get(id).unwrap().tick_pos();
    let near = [at.x + 0.3, at.y + 0.5, at.z];
    mobs.world.session.send_paddle_boat(true, false);
    mobs.world.session.send_move_vehicle(near, 0.0, 0.0, false);
    mobs.wait("the boat followed", |e| {
        let boat = e.get(id).unwrap();
        (boat.tick_pos().x - near[0]).abs() < 1e-3 && boat.appearance.paddles == (true, false)
    });
    assert!(mobs.vehicle_resets.is_empty(), "{:?}", mobs.vehicle_resets);

    let far = [near[0] + 50.0, near[1], near[2]];
    mobs.world.session.send_move_vehicle(far, 0.0, 0.0, false);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while mobs.vehicle_resets.is_empty() {
        assert!(
            std::time::Instant::now() < until,
            "the jump wasn't put back"
        );
        mobs.wait("a tick", |_| true);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let back = mobs.vehicle_resets[0];
    assert!((back[0] - near[0]).abs() < 1e-3, "{back:?} {near:?}");

    mobs.world.session.send_player_input(PlayerInput {
        sneak: true,
        ..Default::default()
    });
    mobs.wait("off the boat, the paddles at rest", |e| {
        e.own_vehicle().is_none() && e.get(id).unwrap().appearance.paddles == (false, false)
    });
}

/// A creative world and the menu the server has open, kept the way the
struct MenuWorld {
    world: TestWorld,
    menu: Option<melon::world::inventory::OpenContainer>,
    /// Every entity the server added: id and `EntityType` id.
    added: Vec<(i32, i32)>,
    /// Every block change the server sent: position and state id.
    changed: Vec<([i32; 3], u16)>,
    carried: pumpkin_data::item_stack::ItemStack,
    /// The plugin channels the server registered.
    channels: Vec<String>,
}

impl MenuWorld {
    fn open(label: &str, username: &str) -> Self {
        let settings = WorldSettings {
            name: "World".to_owned(),
            game_mode: singleplayer::WorldCreationMode::Creative,
            seed: "42".to_owned(),
            difficulty: singleplayer::Difficulty::Easy,
            allow_cheats: true,
            ..Default::default()
        };
        let mut world = TestWorld::open(label, settings, 4, username);
        let joined = std::mem::take(&mut world.joined);
        let mut this = Self {
            world,
            menu: None,
            added: Vec::new(),
            changed: Vec::new(),
            carried: melon::world::inventory::empty(),
            channels: Vec::new(),
        };
        this.apply(joined);
        std::thread::sleep(std::time::Duration::from_secs(1));
        this.pump();
        this
    }

    fn apply(&mut self, events: Vec<WorldEvent>) {
        use melon::network::play::game::{EntityPacket, GameEvent};
        use melon::world::inventory::{ContainerKind, OpenContainer};
        for event in events {
            let event = match event {
                WorldEvent::Game(event) => event,
                WorldEvent::ServerChannels(channels) => {
                    self.channels.extend(channels);
                    continue;
                }
                _ => continue,
            };
            match event {
                GameEvent::Entity(EntityPacket::Add(add)) => self.added.push((add.id, add.kind)),
                GameEvent::BlocksChanged(changes) => self.changed.extend(changes),
                GameEvent::OpenScreen {
                    container,
                    window_type,
                    ..
                } => {
                    let kind = ContainerKind::from_window_type(window_type);
                    self.menu = Some(OpenContainer::new(container, kind, String::new()));
                }
                GameEvent::ContainerContent {
                    container,
                    state_id,
                    slots,
                    ..
                } => {
                    if let Some(menu) = self.menu.as_mut().filter(|m| m.id == container) {
                        menu.set_all(state_id, slots);
                    }
                }
                GameEvent::ContainerSlot {
                    container,
                    state_id,
                    slot,
                    item,
                } => {
                    if let Some(menu) = self.menu.as_mut().filter(|m| m.id == container) {
                        menu.set_slot(state_id, slot, item);
                    }
                }
                GameEvent::ContainerData {
                    container,
                    id,
                    value,
                } => {
                    if let Some(menu) = self.menu.as_mut().filter(|m| m.id == container) {
                        menu.set_data(id as usize, value);
                    }
                }
                GameEvent::MerchantOffers { container, offers } => {
                    if let Some(menu) = self.menu.as_mut().filter(|m| m.id == container) {
                        menu.trades = Some(offers);
                    }
                }
                _ => {}
            }
        }
    }

    fn pump(&mut self) {
        let events = self.world.session.poll();
        self.apply(events);
        assert!(
            self.world.session.ended().is_none(),
            "{:?}",
            self.world.session.ended()
        );
    }

    /// Polls until `done`, for at most 30 s.
    fn wait(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(std::time::Instant::now() < until, "{what} didn't happen");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn menu(&self) -> &melon::world::inventory::OpenContainer {
        self.menu.as_ref().expect("a menu is open")
    }

    /// Gives hotbar slot `n` this item.
    fn give(&mut self, hotbar: i16, item: &'static pumpkin_data::item::Item, count: u8) {
        self.world.session.send_creative_slot(
            36 + hotbar,
            &pumpkin_data::item_stack::ItemStack::new(count, item),
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
        self.pump();
    }

    /// Puts a block `~2 ~ ~` from the player and right-clicks it open.
    fn open_block(&mut self, block: &str, kind: melon::world::inventory::ContainerKind) {
        use melon::world::player::hands::Hand;
        self.world
            .session
            .send_command(&format!("setblock ~2 ~ ~ {block}"));
        std::thread::sleep(std::time::Duration::from_millis(500));
        let at = self.world.spawn.pos;
        let pos = [
            at.0.floor() as i32 + 2,
            at.1.floor() as i32,
            at.2.floor() as i32,
        ];
        self.world
            .session
            .send_use_item_on(Hand::Main, pos, 1, [0.5, 1.0, 0.5], false, 1);
        self.wait(&format!("the {block} opened"), |w| {
            w.menu.as_ref().is_some_and(|m| {
                m.kind == kind && !m.slots.iter().all(|s| s.is_empty())
                    || m.kind == kind && m.slots.len() == kind.slot_count()
            })
        });
    }

    fn close(&mut self) {
        if let Some(menu) = self.menu.take() {
            self.world.session.send_container_close(menu.id);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.pump();
    }

    /// A click as the game sends it: predicted, then with what it changed.
    fn click(&mut self, slot: i16, button: i8, kind: melon::world::inventory::ClickType) {
        let menu = self.menu.as_mut().expect("a menu is open");
        let click = menu.click(slot, button, kind, false, &mut self.carried);
        let changed: Vec<_> = click
            .changed
            .iter()
            .map(|&i| (i, &menu.slots[i as usize]))
            .collect();
        self.world.session.send_container_click(
            menu.id,
            menu.state_id,
            click.slot,
            click.button,
            click.kind,
            &changed,
            &self.carried,
        );
    }

    /// Moves the stack in menu slot `from` to slot `to` (two clicks).
    fn move_stack(&mut self, from: i16, to: i16) {
        use melon::world::inventory::ClickType;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.menu().slots[from as usize].is_empty() {
            self.pump();
            if std::time::Instant::now() > until {
                let all: Vec<_> = self
                    .menu()
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| !s.is_empty())
                    .map(|(i, s)| (i, s.item.registry_key))
                    .collect();
                panic!(
                    "slot {from} stays empty; kind {:?}, items {all:?}",
                    self.menu().kind
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.click(from, 0, ClickType::Pickup);
        self.click(to, 0, ClickType::Pickup);
        std::thread::sleep(std::time::Duration::from_millis(300));
        self.pump();
    }
}

/// The menus the server opens look the way the client lays them out: the
#[test]
#[ignore = "starts an integrated server"]
fn the_servers_menus_agree_with_the_clients() {
    use melon::world::inventory::{ClickType, ContainerKind, loom, stonecutting};
    use pumpkin_data::item::Item;

    let mut w = MenuWorld::open("menus", "Crafter");

    w.give(0, &Item::STONE, 8);
    w.open_block("minecraft:stonecutter", ContainerKind::Stonecutter);
    w.move_stack(29, 0);
    let input = w.menu().slots[0].clone();
    assert_eq!(input.item.registry_key, "stone", "the stone didn't arrive");
    let options = stonecutting::options(&input);
    assert!(options.len() > 3, "stone cuts into several things");
    for index in [0, options.len() - 1] {
        w.world
            .session
            .send_container_button(w.menu().id, index as i32);
        let wanted = options[index].clone();
        w.wait(&format!("recipe {index} in the result slot"), |w| {
            w.menu().slots[1].item.id == wanted.item.id
        });
        assert_eq!(
            w.menu().slots[1].item_count,
            wanted.item_count,
            "recipe {index} gives another count than the client's list"
        );
    }
    w.close();

    w.give(1, &Item::RAW_IRON, 4);
    w.give(2, &Item::COAL, 4);
    w.open_block("minecraft:furnace", ContainerKind::Furnace);
    w.move_stack(31, 0);
    w.move_stack(32, 1);
    let held: Vec<_> = w.menu().slots[..3]
        .iter()
        .map(|s| (s.item.registry_key, s.item_count))
        .collect();
    assert!(
        !w.menu().slots[0].is_empty() && !w.menu().slots[1].is_empty(),
        "furnace slots {held:?}"
    );
    w.wait("the furnace lit", |w| w.menu().data(0) > 0);
    assert_eq!(w.menu().data(3), 200, "a furnace cooks for 200 ticks");
    w.close();

    w.give(3, &Item::WHITE_BANNER, 1);
    w.give(4, &Item::RED_DYE, 1);
    w.open_block("minecraft:loom", ContainerKind::Loom);
    w.move_stack(4 + 27 + 3, 0);
    w.move_stack(4 + 27 + 4, 1);
    let patterns = loom::selectable_patterns(&melon::world::inventory::empty());
    assert!(patterns.len() > 4);
    assert!(w.menu().slots[3].is_empty());
    w.world.session.send_container_button(w.menu().id, 0);
    w.wait("a banner in the result slot", |w| {
        !w.menu().slots[3].is_empty()
    });
    let _ = ClickType::Pickup;
    w.close();

    w.world
        .session
        .send_command("summon minecraft:villager ~ ~ ~3 {NoAI:1b,VillagerData:{profession:\"minecraft:farmer\",level:1,type:\"minecraft:plains\"}}");
    let villager = pumpkin_data::entity::EntityType::VILLAGER.id as i32;
    w.wait("the villager added", |w| {
        w.added.iter().any(|&(_, kind)| kind == villager)
    });
    let id = w
        .added
        .iter()
        .find(|&&(_, kind)| kind == villager)
        .unwrap()
        .0;
    w.world.session.send_interact(
        id,
        melon::world::player::hands::Hand::Main,
        [0.0, 1.0, 0.0],
        false,
    );
    w.wait("the trades open", |w| {
        w.menu
            .as_ref()
            .is_some_and(|m| m.kind == ContainerKind::Merchant && m.trades.is_some())
    });
    let trades = w.menu().trades.as_ref().unwrap();
    assert!(!trades.offers.is_empty(), "a farmer has something to sell");
    assert!(trades.offers.iter().all(|o| !o.result.is_empty()));
    assert!(trades.offers.iter().all(|o| !o.cost_a().is_empty()));
}

/// A block from a zip mod sits after the vanilla ids. Placing it must reach
#[test]
#[ignore = "starts an integrated server"]
fn a_zip_mod_block_can_be_placed() {
    use melon::world::player::hands::Hand;

    let block = pumpkin_data::mod_registry::register_simple_block(
        "melon_test:placed",
        "Placed",
        5.0,
        6.0,
        true,
    )
    .unwrap();
    let item = pumpkin_data::item::Item::from_id(block.item_id).unwrap();
    let mut w = MenuWorld::open("modblock", "Mason");
    w.give(0, item, 4);
    w.world
        .session
        .send_command("setblock ~2 ~ ~ minecraft:stone");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let at = w.world.spawn.pos;
    let target = [
        at.0.floor() as i32 + 2,
        at.1.floor() as i32,
        at.2.floor() as i32,
    ];
    w.world
        .session
        .send_use_item_on(Hand::Main, target, 1, [0.5, 1.0, 0.5], false, 2);
    let state = block.default_state.id.as_u16();
    w.wait("the mod block placed", |w| {
        w.changed.iter().any(|&(_, id)| id == state)
    });
}

/// `melon:swap`: the stone a wall torch hangs on becomes dirt in one step,
/// and the torch stays.
#[test]
#[ignore = "starts an integrated server"]
fn a_swapped_block_keeps_what_hangs_on_it() {
    use melon::network::play::SWAP_CHANNEL;
    use melon::world::player::hands::Hand;
    use pumpkin_data::{Block, item::Item};

    let mut w = MenuWorld::open("swap", "Swapper");
    w.world.session.send_register(&[SWAP_CHANNEL]);
    w.wait("the server takes swaps", |w| {
        w.channels.iter().any(|c| c == SWAP_CHANNEL)
    });
    w.world
        .session
        .send_command("setblock ~2 ~ ~ minecraft:stone");
    let at = w.world.spawn.pos;
    let stone = [
        at.0.floor() as i32 + 2,
        at.1.floor() as i32,
        at.2.floor() as i32,
    ];
    let torch = [stone[0] + 1, stone[1], stone[2]];
    w.wait("the stone stands", |w| {
        w.changed
            .iter()
            .any(|&(pos, id)| pos == stone && block_name(id) == "stone")
    });
    // Up against the stone's east face.
    w.give(0, &Item::TORCH, 4);
    std::thread::sleep(std::time::Duration::from_millis(300));
    w.world
        .session
        .send_use_item_on(Hand::Main, stone, 5, [1.0, 0.5, 0.5], false, 2);
    w.wait("the torch hangs on the stone", |w| {
        w.changed
            .iter()
            .any(|&(pos, id)| pos == torch && block_name(id) == "wall_torch")
    });

    w.give(0, &Item::DIRT, 4);
    std::thread::sleep(std::time::Duration::from_millis(300));
    w.pump();
    w.changed.clear();
    w.world
        .session
        .send_swap_block(Hand::Main, stone, 4, [0.0, 0.5, 0.5], false, 3);
    let dirt = Block::DIRT.default_state.id.as_u16();
    w.wait("the stone became dirt", |w| {
        w.changed.contains(&(stone, dirt))
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    w.pump();
    assert!(
        !w.changed.iter().any(|&(pos, _)| pos == torch),
        "the torch was touched: {:?}",
        w.changed
    );
}

fn block_name(state: u16) -> &'static str {
    pumpkin_data::BlockStateId::new(state)
        .map_or("", |id| pumpkin_data::Block::from_state_id(id).name)
}
