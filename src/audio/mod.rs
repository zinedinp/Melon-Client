//! Sound, through `rodio` (cpal output and symphonia vorbis).

pub mod block_sounds;
mod device;
pub mod hrtf;
pub mod level_sounds;
pub mod library;
pub mod music;
pub mod pcm;
pub mod preview;
pub mod source;
pub mod spatial;
pub mod stats;

use crate::assets::Assets;
use library::{Sound, SoundEvents};
use pcm::PcmBuffer;
use rodio::mixer::Mixer;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Sample, Source};
use source::{SoundSource, Volumes};
use spatial::{Layout, Listener, Placed};
use stats::{AudioStats, Counted, Playing, Pool, Pools, Slot, Watch};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The button click (`SoundEvents.UI_BUTTON_CLICK`).
const CLICK: &str = "ui.button.click";
/// How often a playing sound looks at its source's volume.
const VOLUME_POLL: Duration = Duration::from_millis(10);
/// `SoundEngine.calculatePitch`.
const PITCH_RANGE: (f32, f32) = (0.5, 2.0);

/// A sound to play.
pub struct SoundInstance<'a> {
    /// The sound event, `block.stone.break`.
    pub event: Cow<'a, str>,
    pub source: SoundSource,
    /// Above 1 the sound carries further (`Sound.getAttenuationDistance`).
    pub volume: f32,
    pub pitch: f32,
    /// Where it is heard from, in the world.
    pub position: Option<[f64; 3]>,
}

impl<'a> SoundInstance<'a> {
    /// The event at full volume and normal pitch, on its default source.
    pub fn of(event: impl Into<Cow<'a, str>>) -> Self {
        let event = event.into();
        Self {
            source: SoundSource::for_event(&event),
            event,
            volume: 1.0,
            pitch: 1.0,
            position: None,
        }
    }

    pub fn on(mut self, source: SoundSource) -> Self {
        self.source = source;
        self
    }

    pub fn volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    pub fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    /// Heard from this place in the world.
    pub fn at(mut self, position: [f64; 3]) -> Self {
        self.position = Some(position);
        self
    }
}

/// What each source is worth right now (volume times master), shared with the playing sounds.
#[derive(Default)]
struct Gains([AtomicU32; 11]);

impl Gains {
    fn get(&self, source: SoundSource) -> f32 {
        f32::from_bits(self.0[source.index()].load(Ordering::Relaxed))
    }

    fn set(&self, volumes: &Volumes) {
        for source in SoundSource::ALL {
            let gain = volumes.final_volume(source);
            self.0[source.index()].store(gain.to_bits(), Ordering::Relaxed);
        }
    }
}

/// `MELON_SOUND_SOURCE=<name>` plays every sound on that source, to try its slider.
const FORCED_SOURCE_VAR: &str = "MELON_SOUND_SOURCE";

/// A sound that started with a subtitle, for `SubtitleOverlay.onPlaySound`.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    /// The subtitle's translation key.
    pub subtitle: Arc<str>,
    /// Where it was heard from; `None` for a sound without a place.
    pub at: Option<[f64; 3]>,
    /// `SoundEngine.getRange`: how far off it can still be heard, infinite
    /// for a sound without a place.
    pub range: f32,
}

pub struct AudioEngine {
    _stream: Option<MixerDeviceSink>,
    mixer: Option<Mixer>,
    /// The output's speakers, which every sound is rendered into.
    layout: Layout,
    /// The channels asked for, `None` for what the device prefers.
    channels: Option<u16>,
    /// "Directional Audio": sounds with a place go through the HRTF, and
    /// the output is stereo, as OpenAL Soft makes it then.
    hrtf: Option<&'static hrtf::HrtfStore>,
    device: String,
    /// Whether the subtitles are shown, and so whether [`Heard`]s are kept.
    listening: AtomicBool,
    heard: Mutex<Vec<Heard>>,
    forced_source: Option<SoundSource>,
    listener: Arc<Listener>,
    events: SoundEvents,
    /// The decoded files and who waits for them; replaced on a reload.
    cache: Arc<Cache>,
    loader: Loader,
    pools: Arc<Pools>,
    gains: Arc<Gains>,
    stats: Arc<AudioStats>,
    /// Counts [`Self::stop_all`]s: a sound started under an older count
    /// ends at the mixer's next sample, also one still being decoded.
    generation: Arc<std::sync::atomic::AtomicU64>,
    /// [`Self::set_paused`]: all but the music and the UI hold.
    paused: Arc<std::sync::atomic::AtomicBool>,
}

/// The decoded files (`SoundBufferLibrary.cache`), and the sounds waiting
/// for one that is being decoded.
struct Cache {
    root: PathBuf,
    decoded: Mutex<HashMap<Arc<str>, Option<PcmBuffer>>>,
    waiting: Mutex<HashMap<Arc<str>, Vec<Voice>>>,
}

impl Cache {
    fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            root: root.to_path_buf(),
            decoded: Mutex::default(),
            waiting: Mutex::default(),
        })
    }
}

/// A file to decode, and where to put it.
struct Job {
    cache: Arc<Cache>,
    name: Arc<str>,
    stats: Arc<AudioStats>,
}

/// A few threads that decode the files sounds need: vanilla decodes on a
/// thread pool too (`Util.nonCriticalIoPool`), so a long file does not hold
/// up the short ones behind it. Fixed threads, so a burst of new sounds
/// spawns none.
struct Loader {
    jobs: Option<crossbeam_channel::Sender<Job>>,
}

impl Loader {
    /// Half the cores, two to four.
    fn start() -> Self {
        let threads = std::thread::available_parallelism()
            .map_or(2, |n| n.get() / 2)
            .clamp(2, 4);
        let (jobs, queue) = crossbeam_channel::unbounded::<Job>();
        let mut started = 0;
        for i in 0..threads {
            let queue = queue.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("sound-load-{i}"))
                .spawn(move || {
                    for job in queue {
                        job.run();
                    }
                });
            match spawned {
                Ok(_) => started += 1,
                Err(e) => crate::warn!("no loader thread: {e}"),
            }
        }
        Self {
            jobs: (started > 0).then_some(jobs),
        }
    }

    /// Queues `job`; false when no thread is there to take it.
    fn send(&self, job: Job) -> bool {
        self.jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send(job).is_ok())
    }
}

impl Job {
    /// Decodes the file, then starts every sound that waited for it.
    fn run(self) {
        let buffer = load_buffer(&self.cache, &self.name, &self.stats);
        let voices = self
            .cache
            .waiting
            .lock()
            .ok()
            .and_then(|mut waiting| waiting.remove(&self.name))
            .unwrap_or_default();
        if let Some(buffer) = buffer {
            for voice in voices {
                voice.add(Box::new(buffer.clone()));
            }
        }
    }
}

impl AudioEngine {
    /// Missing device or sound file: playing is a no-op.
    pub fn init(assets: &Assets, device: &str, channels: Option<u16>, directional: bool) -> Self {
        let hrtf = directional.then(hrtf::HrtfStore::default_set).flatten();
        let stream = open_output(device, if hrtf.is_some() { Some(2) } else { channels });
        if stream.is_none() {
            crate::warn!("no output device, sounds stay silent");
        }
        let mixer = stream.as_ref().map(|s| s.mixer().clone());
        let layout = layout_of(stream.as_ref());
        let forced_source = std::env::var(FORCED_SOURCE_VAR)
            .ok()
            .and_then(|name| SoundSource::from_name(&name));
        let gains = Arc::new(Gains::default());
        gains.set(&Volumes::default());
        Self {
            _stream: stream,
            mixer,
            layout,
            channels,
            hrtf,
            device: device.to_owned(),
            listening: AtomicBool::new(false),
            heard: Mutex::default(),
            forced_source,
            listener: Arc::new(Listener::default()),
            events: SoundEvents::load(assets.root()),
            cache: Cache::new(assets.root()),
            loader: Loader::start(),
            pools: Arc::default(),
            gains,
            stats: Arc::default(),
            generation: Arc::default(),
            paused: Arc::default(),
        }
        .preloaded()
    }

    /// `SoundEngine.requestPreload` for every `"preload": true` file.
    fn preloaded(self) -> Self {
        for sound in self.events.preloads() {
            self.queue_decode(&sound.name, None);
        }
        self
    }

    /// The output devices to choose from, by name.
    pub fn reload_sounds(&mut self, assets: &Assets) {
        self.events = SoundEvents::load(assets.root());
        self.cache = Cache::new(assets.root());
        for sound in self.events.preloads() {
            self.queue_decode(&sound.name, None);
        }
    }

    /// `Library.getChannelDebugString`, for F3.
    pub fn channel_debug_string(&self) -> String {
        self.pools.debug_string()
    }

    /// `SoundBufferLibrary.enumerate`: the decoded files held, and their bytes.
    pub fn cache_stats(&self) -> (usize, usize) {
        self.cache.decoded.lock().map_or((0, 0), |decoded| {
            decoded
                .values()
                .flatten()
                .fold((0, 0), |(count, bytes), buffer| {
                    (count + 1, bytes + buffer.size())
                })
        })
    }

    /// The static and the streaming channels in use.
    pub fn channels_used(&self) -> [u32; 2] {
        [Pool::Static, Pool::Streaming].map(|pool| self.pools.used(pool))
    }

    /// Has `name` decoded unless it is already, or already being; `voice`
    /// starts once it is. False when it cannot be.
    fn queue_decode(&self, name: &Arc<str>, voice: Option<Voice>) -> bool {
        let cache = &self.cache;
        if let Ok(decoded) = cache.decoded.lock()
            && let Some(known) = decoded.get(name)
        {
            let Some(buffer) = known.clone() else {
                return false;
            };
            drop(decoded);
            if let Some(voice) = voice {
                voice.add(Box::new(buffer));
            }
            return true;
        }
        let Ok(mut waiting) = cache.waiting.lock() else {
            return false;
        };
        if let Some(voices) = waiting.get_mut(name) {
            voices.extend(voice);
            return true;
        }
        waiting.insert(Arc::clone(name), voice.into_iter().collect());
        drop(waiting);
        let job = Job {
            cache: Arc::clone(cache),
            name: Arc::clone(name),
            stats: Arc::clone(&self.stats),
        };
        if self.loader.send(job) {
            return true;
        }
        if let Ok(mut waiting) = cache.waiting.lock() {
            waiting.remove(name);
        }
        false
    }

    pub fn devices(&self) -> Vec<String> {
        device::output_device_names()
    }

    /// Moves the output to the device with this name (the system default when empty).
    pub fn set_device(&mut self, device: &str) {
        self.device = device.to_owned();
        self.reopen();
    }

    /// Opens the output with `channels` channels (`None`: what the device
    /// prefers, else its own choice when it cannot).
    pub fn set_channels(&mut self, channels: Option<u16>) {
        self.channels = channels;
        self.reopen();
    }

    /// "Directional Audio" on or off; vanilla reloads its sound engine for
    /// it, this reopens the output. Without the data set it stays off.
    pub fn set_directional(&mut self, on: bool) {
        self.hrtf = on.then(hrtf::HrtfStore::default_set).flatten();
        if on && self.hrtf.is_none() {
            crate::warn!("HRTF data unreadable, directional audio stays off");
        }
        self.reopen();
    }

    fn reopen(&mut self) {
        self._stream = None;
        let channels = if self.hrtf.is_some() {
            Some(2)
        } else {
            self.channels
        };
        self._stream = open_output(&self.device, channels);
        self.mixer = self._stream.as_ref().map(|s| s.mixer().clone());
        self.layout = layout_of(self._stream.as_ref());
    }

    /// How many channels the output really has.
    pub fn output_channels(&self) -> usize {
        self.layout.channels()
    }

    /// The camera, for sounds that have a place (`SoundEngine.updateSource`):
    /// call it every frame.
    pub fn set_listener(&self, position: [f64; 3], forward: [f32; 3], up: [f32; 3]) {
        self.listener.set(position, forward, up);
    }

    /// Whether to keep the subtitles of the sounds that start.
    pub fn set_subtitles(&self, on: bool) {
        self.listening.store(on, Ordering::Relaxed);
        if !on && let Ok(mut heard) = self.heard.lock() {
            heard.clear();
        }
    }

    /// The sounds with a subtitle that started since the last call.
    pub fn take_heard(&self) -> Vec<Heard> {
        self.heard
            .lock()
            .map(|mut heard| std::mem::take(&mut *heard))
            .unwrap_or_default()
    }

    /// The sliders changed: every sound, playing or not, follows.
    pub fn set_volumes(&self, volumes: &Volumes) {
        self.gains.set(volumes);
    }

    /// What the engine did so far, for F3 and the benchmark.
    pub fn stats(&self) -> &AudioStats {
        &self.stats
    }

    /// Vanilla `AbstractButton.playDownSound` (`SoundEvents.UI_BUTTON_CLICK`).
    pub fn play_click(&self) {
        self.play(&SoundInstance::of(CLICK));
    }

    /// [`Self::play`], and what the game can ask of it afterwards; `None`
    /// when it does not start (`SoundManager.isActive` is false then).
    pub fn play_watched(&self, instance: &SoundInstance) -> Option<Arc<Playing>> {
        let playing = Arc::new(Playing::default());
        let started = std::time::Instant::now();
        if !self.start(instance, Some(Watch(Arc::clone(&playing)))) {
            return None;
        }
        self.stats.played(started.elapsed().as_nanos() as u64);
        Some(playing)
    }

    /// `SoundManager.stop()` (`SoundEngine.stopAll`): every sound playing,
    /// waiting for its file or streaming goes quiet at once. Vanilla calls
    /// it from `Minecraft.updateLevelInEngines` when the level changes.
    pub fn stop_all(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// `SoundManager.pauseAllExcept(MUSIC, UI)` while the game is paused,
    /// and their resume after: the other sounds hold where they are.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Starts a sound.
    pub fn play(&self, instance: &SoundInstance) {
        let started = std::time::Instant::now();
        if self.start(instance, None) {
            self.stats.played(started.elapsed().as_nanos() as u64);
        }
    }

    /// [`Self::play`]; whether the sound started or is being loaded.
    fn start(&self, instance: &SoundInstance, playing: Option<Watch>) -> bool {
        let Some(mixer) = &self.mixer else {
            return false;
        };
        let Some(sound) = self.events.pick(&instance.event, rand::random::<f32>()) else {
            return false;
        };
        let range = instance.volume.max(1.0) * sound.attenuation_distance;
        if self.listening.load(Ordering::Relaxed)
            && let Some(subtitle) = self.events.subtitle(&instance.event)
            && let Ok(mut heard) = self.heard.lock()
        {
            heard.push(Heard {
                subtitle: Arc::clone(subtitle),
                at: instance.position,
                range: if instance.position.is_some() {
                    range
                } else {
                    f32::INFINITY
                },
            });
        }
        // `channelAccess.createHandle`: the channel is taken before the file
        // loads; with none free the sound does not play.
        let pool = if sound.stream {
            Pool::Streaming
        } else {
            Pool::Static
        };
        let Some(slot) = self.pools.acquire(pool) else {
            return false;
        };
        let volume = instance.volume.clamp(0.0, 1.0) * sound.volume;
        let voice = Voice {
            mixer: mixer.clone(),
            layout: self.layout,
            hrtf: self.hrtf,
            listener: Arc::clone(&self.listener),
            gains: Arc::clone(&self.gains),
            stats: Arc::clone(&self.stats),
            slot,
            playing,
            volume,
            pitch: (instance.pitch * sound.pitch).clamp(PITCH_RANGE.0, PITCH_RANGE.1),
            source: self.forced_source.unwrap_or(instance.source),
            place: instance.position.map(|at| (at, range)),
            generation: {
                let generation = stats::Generation::of(&self.generation);
                let source = self.forced_source.unwrap_or(instance.source);
                if matches!(source, SoundSource::Music | SoundSource::Ui) {
                    generation
                } else {
                    generation.pausing(&self.paused)
                }
            },
        };
        if !sound.stream {
            return self.queue_decode(&sound.name, Some(voice));
        }
        // Music is decoded while it plays, from the file read on a thread of
        // its own (at most eight play at once, the streaming pool).
        let root = self.cache.root.clone();
        let spawned = std::thread::Builder::new()
            .name("sound-stream".to_owned())
            .spawn(move || {
                if let Some(input) = open_stream(&root, &sound) {
                    voice.add(Box::new(input));
                }
            });
        if let Err(e) = spawned {
            crate::warn!("no stream thread for {}: {e}", instance.event);
            return false;
        }
        true
    }
}

/// A sound about to start.
struct Voice {
    mixer: Mixer,
    layout: Layout,
    /// The HRTF a sound with a place is heard through, if it is on.
    hrtf: Option<&'static hrtf::HrtfStore>,
    listener: Arc<Listener>,
    gains: Arc<Gains>,
    stats: Arc<AudioStats>,
    /// Its channel, freed when it ends or never starts.
    slot: Slot,
    /// What the game watches of it, if anything.
    playing: Option<Watch>,
    volume: f32,
    pitch: f32,
    source: SoundSource,
    /// Where it is heard from, and how far it carries.
    place: Option<([f64; 3], f32)>,
    /// The [`AudioEngine::stop_all`] count it was started under.
    generation: stats::Generation,
}

impl Voice {
    fn add(self, input: Box<dyn Source<Item = Sample> + Send>) {
        let input = input.speed(self.pitch);
        let input: Box<dyn Source<Item = Sample> + Send> = match (self.hrtf, self.place) {
            (Some(store), Some((at, range))) => Box::new(hrtf::Binaural::new(
                input,
                store,
                Arc::clone(&self.listener),
                at,
                range,
            )),
            (_, place) => {
                let place = place.map(|(at, range)| (Arc::clone(&self.listener), at, range));
                Box::new(Placed::new(input, self.layout, place))
            }
        };
        let (volume, source, gains) = (self.volume, self.source, self.gains);
        let input =
            input
                .amplify(volume * gains.get(source))
                .periodic_access(VOLUME_POLL, move |s| {
                    s.set_factor(volume * gains.get(source));
                });
        self.mixer.add(Counted::new(
            input,
            self.stats,
            self.slot,
            self.playing,
            self.generation,
        ));
    }
}

/// The decoded file `name`, decoded and remembered the first time; a file
/// that is missing or will not decode is remembered as `None`.
fn load_buffer(cache: &Cache, name: &Arc<str>, stats: &AudioStats) -> Option<PcmBuffer> {
    if let Some(known) = cache.decoded.lock().ok()?.get(name) {
        return known.clone();
    }
    let started = std::time::Instant::now();
    let buffer = decode(&cache.root, name);
    stats.decoded(started.elapsed().as_nanos() as u64);
    let mut decoded = cache.decoded.lock().ok()?;
    let entry = decoded.entry(Arc::clone(name)).or_insert_with(|| {
        if buffer.is_none() {
            crate::warn!("{name}.ogg missing or unreadable");
        }
        buffer
    });
    entry.clone()
}

/// The named device, else the best the system has; with `channels`
/// channels if it takes them, else with its own.
fn open_output(name: &str, channels: Option<u16>) -> Option<MixerDeviceSink> {
    let picked = device::find_output_device(name).or_else(device::pick_output_device);
    let open = |device: &rodio::cpal::Device, count: Option<rodio::ChannelCount>| {
        let builder = DeviceSinkBuilder::from_device(device.clone()).ok()?;
        match count {
            Some(count) => builder.with_channels(count).open_stream(),
            None => builder.open_stream(),
        }
        .inspect_err(|e| crate::warn!("output not open ({e})"))
        .ok()
    };
    let wanted = channels.and_then(rodio::ChannelCount::new);
    let opened = picked.and_then(|device| {
        wanted
            .and_then(|count| open(&device, Some(count)))
            .or_else(|| open(&device, None))
    });
    opened.or_else(|| DeviceSinkBuilder::open_default_sink().ok())
}

/// The speakers of the opened output; stereo while there is none.
fn layout_of(stream: Option<&MixerDeviceSink>) -> Layout {
    Layout::for_channels(stream.map_or(2, |s| s.config().channel_count().get()))
}

/// `JOrbisAudioStream.readAll`: the whole file as 16-bit samples. Like
/// `OpenAlUtil.audioFormatToOpenAl`, only mono and stereo are taken.
fn decode(assets_root: &Path, name: &str) -> Option<PcmBuffer> {
    let bytes = crate::assets::sound(assets_root, name)?;
    let decoder = Decoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (channels, sample_rate) = (decoder.channels(), decoder.sample_rate());
    if channels.get() > 2 {
        crate::warn!("{name}.ogg has {channels} channels, only mono and stereo play");
        return None;
    }
    Some(PcmBuffer::from_samples(channels, sample_rate, decoder))
}

/// Music: decoded while it plays, from the file read whole.
fn open_stream(assets_root: &Path, sound: &Sound) -> Option<Decoder<std::io::Cursor<Vec<u8>>>> {
    let bytes = crate::assets::sound(assets_root, &sound.name)?;
    match Decoder::new(std::io::Cursor::new(bytes)) {
        Ok(decoder) => Some(decoder),
        Err(e) => {
            crate::warn!("{}.ogg unplayable: {e}", sound.name);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode-only: the `.ogg` yields stereo samples without an OS audio device.
    #[test]
    fn click_sound_decodes() {
        let bytes = crate::assets::sound(&crate::assets::default_root(), "random/click_stereo")
            .expect("assets/minecraft/sounds/random/click_stereo.ogg nicht lesbar");
        let mut decoder = Decoder::new(std::io::Cursor::new(bytes))
            .expect("Decoder::new sollte click_stereo.ogg dekodieren");
        assert_eq!(
            decoder.channels().get(),
            2,
            "click_stereo.ogg sollte stereo sein"
        );
        let sample_count = (0..).take_while(|_| decoder.next().is_some()).count();
        assert!(
            sample_count > 1000,
            "erwarte mehrere tausend Samples, bekam {sample_count}"
        );
    }

    #[test]
    fn gains_follow_the_volumes() {
        let gains = Gains::default();
        let mut volumes = Volumes::default();
        volumes.set(SoundSource::Master, 0.5);
        volumes.set(SoundSource::Music, 0.5);
        gains.set(&volumes);
        assert_eq!(gains.get(SoundSource::Ui), 0.5);
        assert_eq!(gains.get(SoundSource::Music), 0.25);
    }

    #[test]
    fn an_instance_defaults_to_its_events_source() {
        let sound = SoundInstance::of("block.stone.break");
        assert_eq!(sound.source, SoundSource::Blocks);
        assert_eq!(sound.on(SoundSource::Music).source, SoundSource::Music);
    }
}
