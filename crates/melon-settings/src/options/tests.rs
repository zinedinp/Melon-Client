use super::*;

#[test]
fn reads_back_what_it_writes() {
    let config = Config {
        lang: Language::from_code("de_de").unwrap_or(Language::EnUs),
        fov: 95,
        render_distance: 20,
        simulation_distance: 8,
        cull: CullSettings {
            frustum: true,
            occlusion: false,
            entity_distance: 2.25,
        },
        graphics_preset: GraphicsPreset::Custom,
        smooth_lighting: false,
        cutout_leaves: false,
        chunk_builder: ChunkBuilder::Nearby,
        biome_blend: 5,
        clouds: CloudStatus::Fast,
        cloud_range: 32,
        weather_radius: 5,
        vignette: false,
        screen_effect_scale: 0.5,
        fov_effect_scale: 0.25,
        damage_tilt_strength: 0.5,
        gui_scale: 3,
        fullscreen: true,
        vsync: false,
        pause_on_lost_focus: false,
        debug_gui_scale: 2,
        present_mode: PresentMode::Mailbox,
        max_fps: 60,
        inactivity_fps_limit: InactivityFpsLimit::Minimized,
        gamma: 1.0,
        chat: ChatSettings {
            visibility: ChatVisibility::System,
            colors: false,
            opacity: 0.25,
            delay: 1.5,
            save_drafts: true,
            ..ChatSettings::default()
        },
        sound_volumes: {
            let mut volumes = Volumes::default();
            volumes.set(SoundSource::Master, 0.5);
            volumes.set(SoundSource::Records, 0.0);
            volumes.set(SoundSource::Ui, 0.25);
            volumes
        },
        sound_device: "Built-in \"Audio\"".to_owned(),
        show_subtitles: true,
        directional_audio: true,
        speaker_layout: SpeakerLayout::Surround51,
        music_frequency: MusicFrequency::Constant,
        music_toast: MusicToast::PauseAndToast,
        input: InputSettings {
            sensitivity: 0.75,
            invert_y: true,
            toggle_crouch: true,
            toggle_sprint: true,
            auto_jump: true,
            sprint_window: 3,
            binds: {
                let mut binds = crate::input::KeyBinds::default();
                binds.set(Action::Forward, InputBind::parse("key.keyboard.up"));
                binds
            },
        },
        model_parts: {
            let mut parts = ShownParts::default();
            parts.set(PlayerModelPart::Cape, false);
            parts.set(PlayerModelPart::RightPantsLeg, false);
            parts
        },
        main_hand: Arm::Left,
        bob_view: false,
        high_contrast_block_outline: true,
        attack_indicator: AttackIndicator::Hotbar,
        particles: ParticleStatus::Decreased,
        resource_packs: vec!["vanilla".to_owned(), "file/Faithful 32x.zip".to_owned()],
        incompatible_resource_packs: vec!["file/Old \"Pack\"".to_owned()],
    };
    assert_eq!(Config::parse(&config.serialize()), config);
}

#[test]
fn sound_volumes_use_vanillas_keys() {
    let config =
        Config::parse("soundCategory_music:0.3\nsoundCategory_record:9\nsoundCategory_bogus:0.1\n");
    assert_eq!(config.sound_volumes.get(SoundSource::Music), 0.3);
    assert_eq!(config.sound_volumes.get(SoundSource::Records), 1.0);
    assert_eq!(config.sound_volumes.get(SoundSource::Master), 1.0);
    let text = Config::default().serialize();
    assert!(text.contains("soundCategory_master:1.0\n"));
    assert!(text.contains("soundCategory_record:1.0\n"));
}

#[test]
fn reads_vanillas_encoding_and_clamps() {
    let config = Config::parse(
        "fov:1.0\nrenderDistance:99\nentityDistanceScaling:0.1\nchatOpacity:7\nchatVisibility:2\nbogus\n",
    );
    assert_eq!(config.fov, 110);
    assert_eq!(config.render_distance, 32);
    assert_eq!(Config::parse("simulationDistance:2").simulation_distance, 5);
    assert_eq!(Config::parse("").simulation_distance, 12);
    assert_eq!(config.cull.entity_distance, 0.5);
    assert_eq!(config.chat.opacity, 1.0);
    assert_eq!(config.chat.visibility, ChatVisibility::Hidden);
    assert_eq!(Config::parse("fov:0.0").fov, 70);
    assert_eq!(Config::parse("").render_distance, 12);
    assert_eq!(Config::parse("").chat.height_unfocused, 0.4375);
    assert_eq!(Config::parse("maxFps:1000").framerate_limit(), None);
    assert_eq!(Config::parse("maxFps:3").framerate_limit(), Some(10));
    assert!(Config::parse("").vsync);
    assert_eq!(Config::parse("").present_mode, PresentMode::Auto);
    assert_eq!(
        Config::parse("presentMode:mailbox").present_mode,
        PresentMode::Mailbox
    );
    assert_eq!(
        Config::parse("presentMode:bogus").present_mode,
        PresentMode::Auto
    );
    assert_eq!(Config::parse("").gamma, GAMMA_DEFAULT);
    assert_eq!(Config::parse("gamma:0").gamma, 0.0);
    assert_eq!(Config::parse("gamma:2").gamma, 1.0);
    assert_eq!(Config::parse("gamma:0.5").gamma, 0.5);
    assert_eq!(
        Config::parse("").input.sensitivity,
        input::SENSITIVITY_DEFAULT
    );
    assert_eq!(
        Config::parse("mouseSensitivity:0.25").input.sensitivity,
        0.25
    );
    assert!(Config::parse("invertYMouse:true").input.invert_y);
    assert_eq!(Config::parse("sprintWindow:99").input.sprint_window, 10);
    assert_eq!(
        Config::parse("key_key.forward:key.keyboard.up")
            .input
            .binds
            .get(Action::Forward)
            .name(),
        "key.keyboard.up"
    );
    assert_eq!(
        Config::parse("").input.binds.get(Action::Attack).name(),
        "key.mouse.left"
    );
    assert!(Config::parse("").smooth_lighting);
    assert!(!Config::parse("ao:false").smooth_lighting);
    assert_eq!(Config::parse("").graphics_preset, GraphicsPreset::Fancy);
    assert_eq!(
        Config::parse("graphicsPreset:\"fast\"").graphics_preset,
        GraphicsPreset::Fast
    );
    assert_eq!(
        Config::parse("graphicsPreset:bogus").graphics_preset,
        GraphicsPreset::Fancy
    );
    assert_eq!(Config::parse("").chunk_builder, ChunkBuilder::None);
    assert_eq!(
        Config::parse("prioritizeChunkUpdates:1").chunk_builder,
        ChunkBuilder::PlayerAffected
    );
    assert_eq!(
        Config::parse("prioritizeChunkUpdates:5").chunk_builder,
        ChunkBuilder::Nearby
    );
    assert_eq!(
        Config::parse("prioritizeChunkUpdates:-1").chunk_builder,
        ChunkBuilder::Nearby
    );
}

#[test]
fn presets_set_vanillas_values() {
    let mut config = Config::default();
    GraphicsPreset::Fast.apply(&mut config);
    assert_eq!(
        (
            config.render_distance,
            config.chunk_builder,
            config.simulation_distance,
            config.smooth_lighting,
            config.cull.entity_distance
        ),
        (8, ChunkBuilder::None, 6, false, 0.75)
    );
    assert_eq!(config.biome_blend, 1);
    assert_eq!(config.clouds, CloudStatus::Fast);
    assert_eq!(config.cloud_range, 32);
    assert_eq!(config.weather_radius, 5);
    GraphicsPreset::Fabulous.apply(&mut config);
    assert_eq!(
        (
            config.render_distance,
            config.chunk_builder,
            config.smooth_lighting,
            config.cull.entity_distance
        ),
        (32, ChunkBuilder::PlayerAffected, true, 1.25)
    );
    assert_eq!(config.biome_blend, 2);
    assert_eq!(config.clouds, CloudStatus::Fancy);
    assert_eq!(config.cloud_range, 128);
    assert_eq!(config.weather_radius, 10);
    let before = config.clone();
    GraphicsPreset::Custom.apply(&mut config);
    assert_eq!(config, before);
}

#[test]
fn the_screen_options_read_and_write() {
    let config = Config::parse(
        "biomeBlendRadius:9\nvignette:false\nscreenEffectScale:0.25\nfovEffectScale:0",
    );
    assert_eq!(config.biome_blend, 7);
    assert!(!config.vignette);
    assert_eq!(config.screen_effect_scale, 0.25);
    assert_eq!(config.fov_effect_scale, 0.0);
    let defaults = Config::parse("");
    assert_eq!(defaults.biome_blend, 2);
    assert!(defaults.vignette);
    assert_eq!(defaults.screen_effect_scale, 1.0);
    let text = config.serialize();
    assert!(text.contains("biomeBlendRadius:7\n"));
    let clouds = Config::parse("renderClouds:\"fast\"\ncloudRange:40\nweatherRadius:4");
    assert_eq!(clouds.clouds, crate::world::clouds::CloudStatus::Fast);
    assert_eq!(clouds.cloud_range, 40);
    assert_eq!(clouds.weather_radius, 4);
    assert!(text.contains("vignette:false\n"));
}
