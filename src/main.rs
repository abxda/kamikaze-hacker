// Some def fields (enemy radius, display names, beam lifetime) are kept for
// completeness / future use but not all are read yet.
#![allow(dead_code)]

// =====================================================================
//  KAMIKAZE HACKER  —  cyberpunk tower defense / crowd-runner hybrid
//  by Dr. Coronado x Claude
//  Rust + macroquad  ->  WebAssembly (wasm32-unknown-unknown)
//
//  100% self-contained / legally clean:
//    * no Google Fonts, no React CDN, no network calls at runtime
//    * only dependency is macroquad (MIT / Apache-2.0)
//    * all art, music and SFX generated in code; no trademarked names
//
//  Improvements over the original prototype:
//    * RANDOMNESS  : wave jitter, random ELITE mutations, critical hits,
//                    random anomaly tiles, randomized parallax rain.
//    * MORE COLOR  : HSV hue-cycling "dimensional rift", richer palettes,
//                    rainbow boss, neon gradients on path + particles.
//    * DIFFICULTY  : steeper HP scaling, elite enemies, periodic
//                    DIMENSION SHIFT events that speed/buff every virus.
//    * MULTI-DIM   : 3 parallax depth layers of glyph rain + a global
//                    "dimension" phase that recolors the whole board.
// =====================================================================

use macroquad::prelude::*;
use macroquad::rand::gen_range;
use macroquad::audio::{load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound};
use std::collections::HashMap;

fn window_conf() -> Conf {
    Conf {
        window_title: "Kamikaze Hacker".to_owned(),
        // high_dpi MUST stay false: with it on, the JS glue scales touch/mouse
        // coords by devicePixelRatio while the reported screen size is not, so on
        // phones (DPR 2-3) every tap lands far outside the play area. Off => both
        // sides use the same CSS-pixel units and taps line up everywhere.
        high_dpi: false,
        window_resizable: true,
        window_width: 1100,
        window_height: 620,
        ..Default::default()
    }
}

// ----------------------------------------------------------- color helpers
fn col(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}
fn cola(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}
fn with_a(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(lerp(a.r, b.r, t), lerp(a.g, b.g, t), lerp(a.b, b.b, t), lerp(a.a, b.a, t))
}
// h in [0,1), s,v in [0,1]
fn hsv(h: f32, s: f32, v: f32) -> Color {
    let h = (h.fract() + 1.0).fract() * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color::new(r, g, b, 1.0)
}

// ----------------------------------------------------------- procedural audio
// Everything is synthesized in code and packed into in-memory WAV buffers, so the
// build stays self-contained (no external audio files, no licensing concerns).
const SR: u32 = 22_050;

fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n - 69) as f32 / 12.0)
}

// wave: 0=square, 1=triangle, 2=sine, 3=noise
fn synth_tone(buf: &mut [f32], start: f32, dur: f32, freq: f32, wave: u8, amp: f32) {
    let n0 = (start * SR as f32) as usize;
    let nd = (dur * SR as f32) as usize;
    for i in 0..nd {
        let idx = n0 + i;
        if idx >= buf.len() {
            break;
        }
        let ph = freq * (idx as f32 / SR as f32); // cycles
        let frac = ph.fract();
        let s = match wave {
            0 => {
                if frac < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            1 => 2.0 * (2.0 * frac - 1.0).abs() - 1.0,
            2 => (ph * std::f32::consts::TAU).sin(),
            _ => gen_range(-1.0f32, 1.0f32),
        };
        let t = i as f32 / nd as f32;
        let env = (1.0 - t).max(0.0);
        let env = env * env; // exponential-ish decay
        buf[idx] += s * amp * env;
    }
}

fn to_wav(buf: &[f32]) -> Vec<u8> {
    let data_len = (buf.len() * 2) as u32;
    let mut v: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVE");
    v.extend_from_slice(b"fmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&SR.to_le_bytes());
    v.extend_from_slice(&(SR * 2).to_le_bytes()); // byte rate
    v.extend_from_slice(&2u16.to_le_bytes()); // block align
    v.extend_from_slice(&16u16.to_le_bytes()); // bits
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for &x in buf {
        let s = (x.clamp(-1.0, 1.0) * 32767.0 * 0.9) as i16;
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

// 4-bar chord progression (Am - F - C - G), used by both music layers so they
// stay harmonically in sync. The track is split into two STEMS that the game
// crossfades by intensity: a calm instrumental "bed" (pads + arpeggio) that always
// plays, and a driving "drive" layer (drums + bass + lead) that swells during waves.
const SPB: f32 = 0.13; // one 16th note
const MBARS: usize = 4;
const CHORD_BASS: [i32; 4] = [45, 41, 36, 43]; // A2 F2 C2 G2
const CHORD_TRIAD: [[i32; 3]; 4] = [
    [57, 60, 64], // Am
    [53, 57, 60], // F
    [48, 52, 55], // C
    [55, 59, 62], // G
];
const LEAD_LINE: [i32; 16] = [
    64, 67, 72, 67, 60, 65, 69, 65, 55, 60, 64, 60, 62, 67, 71, 67,
];

fn music_buf() -> Vec<f32> {
    vec![0.0f32; (SPB * (MBARS * 16) as f32 * SR as f32) as usize + 1]
}

// Calm instrumental bed: warm sine pad on each chord + a gentle triangle arpeggio.
fn build_bed() -> Vec<u8> {
    let mut buf = music_buf();
    for bar in 0..MBARS {
        let triad = CHORD_TRIAD[bar];
        let bar_t = (bar * 16) as f32 * SPB;
        for &n in &triad {
            synth_tone(&mut buf, bar_t, SPB * 15.5, midi(n - 12), 2, 0.05); // pad
        }
        let mut s = 0;
        while s < 16 {
            let note = triad[(s / 2) % 3] + 12;
            synth_tone(&mut buf, (bar * 16 + s) as f32 * SPB, SPB * 1.6, midi(note), 1, 0.07);
            s += 2;
        }
    }
    to_wav(&buf)
}

// Driving layer: four-on-the-floor kit, root bass on 8ths, and a square lead hook.
fn build_drive() -> Vec<u8> {
    let mut buf = music_buf();
    for bar in 0..MBARS {
        let root = CHORD_BASS[bar];
        for s in 0..16 {
            let t = (bar * 16 + s) as f32 * SPB;
            if s % 4 == 0 {
                synth_tone(&mut buf, t, 0.16, 58.0, 2, 0.55);
                synth_tone(&mut buf, t, 0.012, 200.0, 3, 0.30);
            }
            if s % 2 == 1 {
                synth_tone(&mut buf, t, 0.035, 8000.0, 3, 0.09);
            }
            if s == 4 || s == 12 {
                synth_tone(&mut buf, t, 0.09, 1200.0, 3, 0.20);
            }
            if s % 2 == 0 {
                synth_tone(&mut buf, t, SPB * 0.9, midi(root + 12), 1, 0.18);
            }
            if s % 4 == 0 {
                let ln = LEAD_LINE[bar * 4 + s / 4];
                synth_tone(&mut buf, t, SPB * 1.5, midi(ln), 0, 0.085);
            }
        }
    }
    to_wav(&buf)
}

fn build_sfx(name: &str) -> Vec<u8> {
    // (start, dur, freq, wave, amp)
    let seq: Vec<(f32, f32, f32, u8, f32)> = match name {
        "shoot" => vec![(0.0, 0.07, 720.0, 0, 0.32), (0.02, 0.05, 300.0, 0, 0.2)],
        "boom" => vec![(0.0, 0.22, 200.0, 3, 0.5)],
        "bigboom" => vec![(0.0, 0.42, 120.0, 3, 0.6), (0.0, 0.42, 60.0, 0, 0.25)],
        "place" => vec![(0.0, 0.08, 440.0, 0, 0.22), (0.05, 0.08, 660.0, 0, 0.22), (0.1, 0.1, 880.0, 0, 0.22)],
        "upgrade" => vec![(0.0, 0.09, 523.0, 0, 0.22), (0.05, 0.09, 659.0, 0, 0.22), (0.1, 0.09, 784.0, 0, 0.22), (0.15, 0.12, 1047.0, 0, 0.22)],
        "sell" => vec![(0.0, 0.09, 660.0, 1, 0.22), (0.05, 0.09, 440.0, 1, 0.22), (0.1, 0.11, 300.0, 1, 0.22)],
        "frost" => vec![(0.0, 0.16, 820.0, 2, 0.22), (0.05, 0.14, 420.0, 2, 0.18)],
        "core" => vec![(0.0, 0.26, 150.0, 0, 0.34), (0.0, 0.26, 90.0, 3, 0.18)],
        "wave" => vec![(0.0, 0.14, 330.0, 0, 0.24), (0.08, 0.14, 392.0, 0, 0.24), (0.16, 0.16, 494.0, 0, 0.24)],
        "win" => vec![(0.0, 0.16, 523.0, 0, 0.26), (0.12, 0.16, 659.0, 0, 0.26), (0.24, 0.16, 784.0, 0, 0.26), (0.36, 0.16, 1047.0, 0, 0.26), (0.48, 0.24, 1319.0, 0, 0.26)],
        "lose" => vec![(0.0, 0.22, 392.0, 0, 0.28), (0.16, 0.22, 330.0, 0, 0.28), (0.32, 0.22, 262.0, 0, 0.28), (0.48, 0.3, 196.0, 0, 0.28)],
        _ => vec![(0.0, 0.1, 440.0, 0, 0.2)],
    };
    let end = seq.iter().map(|t| t.0 + t.1).fold(0.0f32, f32::max);
    let mut buf = vec![0.0f32; (end * SR as f32) as usize + 1];
    for (start, dur, freq, wave, amp) in seq {
        synth_tone(&mut buf, start, dur, freq, wave, amp);
    }
    to_wav(&buf)
}

const SFX_NAMES: &[&str] = &["shoot", "boom", "bigboom", "place", "upgrade", "sell", "frost", "core", "wave", "win", "lose"];

// ----------------------------------------------------------- enums
#[derive(Clone, Copy, PartialEq)]
enum TKind {
    Firewall,
    Antivirus,
    Ice,
    LogicBomb,
    Proxy,
}
#[derive(Clone, Copy, PartialEq)]
enum EKind {
    Bit,
    Trojan,
    Worm,
    Ddos,
    Corruptor,
    Ransomware,
}
#[derive(Clone, Copy, PartialEq)]
enum Proj {
    Bolt,
    Beam,
    Frost,
    Mortar,
    Aura,
}
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Select,
    Play,
}
// Crowd-runner style multiplier — but it's YOURS. A FORK orb drifts down a lane;
// tap it to grow your swarm of allied drones (the "+N" adds, the "x2" doubles).
#[derive(Clone, Copy, PartialEq)]
enum OrbKind {
    Add,    // +2 drones
    Double, // x2 drones
}

// ----------------------------------------------------------- defs
#[derive(Clone, Copy)]
struct TowerDef {
    glyph: &'static str,
    name_en: &'static str,
    name_es: &'static str,
    c0: Color,
    c1: Color,
    c2: Color,
    range: [f32; 3],
    dmg: [f32; 3],
    cd: [f32; 3],
    cost: [i32; 3],
    proj: Proj,
    slow: [f32; 3],
    slow_dur: [f32; 3],
    splash: [f32; 3],
    buff_dmg: [f32; 3],
    buff_range: [f32; 3],
}

fn tdef(k: TKind) -> TowerDef {
    match k {
        TKind::Firewall => TowerDef {
            glyph: "#", name_en: "Firewall", name_es: "Firewall",
            c0: col(0x0c, 0x3b, 0x1e), c1: col(0x1f, 0x9c, 0x4d), c2: col(0x7d, 0xff, 0xb0),
            range: [2.3, 2.6, 2.9], dmg: [5.0, 9.0, 16.0], cd: [0.42, 0.34, 0.27], cost: [50, 55, 90],
            proj: Proj::Bolt, slow: [0.0; 3], slow_dur: [0.0; 3], splash: [0.0; 3],
            buff_dmg: [0.0; 3], buff_range: [0.0; 3],
        },
        TKind::Antivirus => TowerDef {
            glyph: "+", name_en: "Antivirus", name_es: "Antivirus",
            c0: col(0x06, 0x32, 0x3f), c1: col(0x13, 0xa8, 0xc8), c2: col(0x9b, 0xf0, 0xff),
            range: [3.5, 3.9, 4.3], dmg: [17.0, 30.0, 50.0], cd: [0.78, 0.7, 0.6], cost: [90, 95, 140],
            proj: Proj::Beam, slow: [0.0; 3], slow_dur: [0.0; 3], splash: [0.0; 3],
            buff_dmg: [0.0; 3], buff_range: [0.0; 3],
        },
        TKind::Ice => TowerDef {
            glyph: "*", name_en: "ICE", name_es: "ICE",
            c0: col(0x06, 0x3a, 0x52), c1: col(0x2a, 0xa6, 0xff), c2: col(0xbf, 0xe6, 0xff),
            range: [2.6, 2.9, 3.2], dmg: [1.0, 2.0, 4.0], cd: [0.9, 0.8, 0.7], cost: [80, 85, 120],
            proj: Proj::Frost, slow: [0.45, 0.55, 0.66], slow_dur: [1.3, 1.6, 2.0], splash: [0.0; 3],
            buff_dmg: [0.0; 3], buff_range: [0.0; 3],
        },
        TKind::LogicBomb => TowerDef {
            glyph: "%", name_en: "Logic Bomb", name_es: "Bomba",
            c0: col(0x3a, 0x26, 0x06), c1: col(0xe0, 0x8a, 0x13), c2: col(0xff, 0xd8, 0x4d),
            range: [2.9, 3.1, 3.4], dmg: [34.0, 56.0, 92.0], cd: [1.6, 1.45, 1.3], cost: [120, 120, 170],
            proj: Proj::Mortar, slow: [0.0; 3], slow_dur: [0.0; 3], splash: [1.35, 1.55, 1.8],
            buff_dmg: [0.0; 3], buff_range: [0.0; 3],
        },
        TKind::Proxy => TowerDef {
            glyph: "@", name_en: "Proxy", name_es: "Proxy",
            c0: col(0x3a, 0x06, 0x3a), c1: col(0xc8, 0x1f, 0xb4), c2: col(0xff, 0x8b, 0xe0),
            range: [1.9, 2.1, 2.4], dmg: [0.0; 3], cd: [1.0, 1.0, 1.0], cost: [70, 75, 110],
            proj: Proj::Aura, slow: [0.0; 3], slow_dur: [0.0; 3], splash: [0.0; 3],
            buff_dmg: [0.22, 0.34, 0.5], buff_range: [0.1, 0.16, 0.24],
        },
    }
}

#[derive(Clone, Copy)]
struct EnemyDef {
    hp: f32,
    spd: f32,
    reward: i32,
    leak: i32,
    r: f32,
    draw: f32,
    split: i32,
    aura: bool,
    armor: f32,
    boss: bool,
    base: Color,
    accent: Color,
    name_en: &'static str,
    name_es: &'static str,
}

fn edef(k: EKind) -> EnemyDef {
    match k {
        EKind::Bit => EnemyDef {
            hp: 10.0, spd: 2.6, reward: 3, leak: 1, r: 0.34, draw: 0.74, split: 0, aura: false,
            armor: 0.0, boss: false, base: col(0x15, 0x7a, 0x35), accent: col(0x39, 0xff, 0x14),
            name_en: "Bit", name_es: "Bit",
        },
        EKind::Trojan => EnemyDef {
            hp: 86.0, spd: 1.05, reward: 12, leak: 3, r: 0.46, draw: 1.04, split: 0, aura: false,
            armor: 0.0, boss: false, base: col(0x4a, 0x1f, 0x78), accent: col(0x9a, 0x4d, 0xff),
            name_en: "Trojan", name_es: "Troyano",
        },
        EKind::Worm => EnemyDef {
            hp: 30.0, spd: 1.85, reward: 6, leak: 2, r: 0.4, draw: 0.92, split: 2, aura: false,
            armor: 0.0, boss: false, base: col(0x0f, 0x6b, 0x2a), accent: col(0x3b, 0xdb, 0x6a),
            name_en: "Worm", name_es: "Gusano",
        },
        EKind::Ddos => EnemyDef {
            hp: 5.0, spd: 3.35, reward: 1, leak: 1, r: 0.26, draw: 0.52, split: 0, aura: false,
            armor: 0.0, boss: false, base: col(0x0e, 0x8a, 0xa8), accent: col(0x18, 0xe0, 0xff),
            name_en: "DDoS", name_es: "DDoS",
        },
        EKind::Corruptor => EnemyDef {
            hp: 52.0, spd: 1.4, reward: 16, leak: 2, r: 0.42, draw: 0.92, split: 0, aura: true,
            armor: 0.0, boss: false, base: col(0x8a, 0x15, 0x68), accent: col(0xff, 0x2b, 0xd6),
            name_en: "Corruptor", name_es: "Corruptor",
        },
        EKind::Ransomware => EnemyDef {
            hp: 1500.0, spd: 0.82, reward: 200, leak: 12, r: 0.7, draw: 1.9, split: 0, aura: false,
            armor: 4.0, boss: true, base: col(0x7a, 0x1a, 0x1a), accent: col(0xff, 0x3b, 0x30),
            name_en: "Ransomware", name_es: "Ransomware",
        },
    }
}

struct MapDef {
    name_en: &'static str,
    name_es: &'static str,
    cols: i32,
    rows: i32,
    bytes: i32,
    lives: i32,
    waves: i32,
    towers: Vec<TKind>,
    pools: Vec<EKind>,
    paths: Vec<Vec<(f32, f32)>>,
    core: (f32, f32),
    hint_en: &'static str,
    hint_es: &'static str,
}

fn build_maps() -> Vec<MapDef> {
    use EKind::*;
    use TKind::*;
    vec![
        MapDef {
            name_en: "BOOT SECTOR", name_es: "SECTOR DE ARRANQUE", cols: 20, rows: 11,
            bytes: 180, lives: 20, waves: 8, towers: vec![Firewall], pools: vec![Bit],
            paths: vec![vec![(-1.0, 5.0), (21.0, 5.0)]], core: (19.4, 5.0),
            hint_en: "Straight data line. Bits only - learn to place Firewalls.",
            hint_es: "Linea recta. Solo Bits - aprende a colocar Firewalls.",
        },
        MapDef {
            name_en: "DATA STREAM", name_es: "FLUJO DE DATOS", cols: 20, rows: 11,
            bytes: 205, lives: 20, waves: 9, towers: vec![Firewall, Antivirus], pools: vec![Bit, Trojan],
            paths: vec![
                vec![(-1.0, 2.0), (15.0, 2.0), (15.0, 5.0), (4.0, 5.0), (4.0, 8.0), (21.0, 8.0)],
                vec![(-1.0, 8.0), (3.0, 8.0), (3.0, 2.0), (11.0, 2.0), (11.0, 6.0), (18.0, 6.0), (18.0, 8.0), (21.0, 8.0)],
            ],
            core: (19.4, 8.0),
            hint_en: "S-curve. Trojans tank - the Antivirus sniper helps.",
            hint_es: "Curva en S. Llegan Troyanos - usa el Antivirus.",
        },
        MapDef {
            name_en: "THE MAINFRAME", name_es: "EL MAINFRAME", cols: 20, rows: 11,
            bytes: 225, lives: 20, waves: 10, towers: vec![Firewall, Antivirus, Ice],
            pools: vec![Bit, Trojan, Worm],
            paths: vec![
                vec![(-1.0, 5.0), (5.0, 5.0), (5.0, 2.0), (14.0, 2.0), (14.0, 5.0), (21.0, 5.0)],
                vec![(-1.0, 5.0), (5.0, 5.0), (5.0, 8.0), (14.0, 8.0), (14.0, 5.0), (21.0, 5.0)],
            ],
            core: (19.4, 5.0),
            hint_en: "Two routes merge. Worms split - chill them with ICE.",
            hint_es: "Dos rutas se unen. Los Gusanos se dividen - usa ICE.",
        },
        MapDef {
            name_en: "FIREWALL BREACH", name_es: "BRECHA", cols: 20, rows: 11,
            bytes: 250, lives: 20, waves: 10, towers: vec![Firewall, Antivirus, Ice, LogicBomb],
            pools: vec![Bit, Trojan, Worm, Ddos],
            paths: vec![vec![
                (-1.0, 0.0), (18.0, 0.0), (18.0, 10.0), (1.0, 10.0), (1.0, 2.0),
                (16.0, 2.0), (16.0, 8.0), (5.0, 8.0), (5.0, 5.0), (11.0, 5.0),
            ]],
            core: (11.0, 5.0),
            hint_en: "Spiral, tight space. DDoS swarms - deploy Logic Bombs.",
            hint_es: "Espiral estrecha. Enjambres DDoS - usa Bombas Logicas.",
        },
        MapDef {
            name_en: "DEEP NET", name_es: "RED PROFUNDA", cols: 20, rows: 11,
            bytes: 270, lives: 22, waves: 11, towers: vec![Firewall, Antivirus, Ice, LogicBomb, Proxy],
            pools: vec![Bit, Trojan, Worm, Ddos, Corruptor],
            paths: vec![
                vec![
                    (-1.0, 1.0), (18.0, 1.0), (18.0, 3.0), (1.0, 3.0), (1.0, 5.0),
                    (18.0, 5.0), (18.0, 7.0), (1.0, 7.0), (1.0, 9.0), (21.0, 9.0),
                ],
                vec![(-1.0, 5.0), (16.0, 5.0), (16.0, 2.0), (6.0, 2.0), (6.0, 9.0), (21.0, 9.0)],
            ],
            core: (19.4, 9.0),
            hint_en: "Long maze. Corruptors shield virii - pair with Proxy.",
            hint_es: "Laberinto largo. Corruptores escudan - combina con Proxy.",
        },
        MapDef {
            name_en: "ROOT ACCESS", name_es: "ACCESO ROOT", cols: 20, rows: 11,
            bytes: 330, lives: 25, waves: 12, towers: vec![Firewall, Antivirus, Ice, LogicBomb, Proxy],
            pools: vec![Bit, Trojan, Worm, Ddos, Corruptor, Ransomware],
            paths: vec![
                vec![
                    (-1.0, 5.0), (4.0, 5.0), (4.0, 1.0), (15.0, 1.0), (15.0, 9.0),
                    (6.0, 9.0), (6.0, 5.0), (21.0, 5.0),
                ],
                vec![(-1.0, 5.0), (3.0, 5.0), (3.0, 9.0), (12.0, 9.0), (12.0, 2.0), (18.0, 2.0), (18.0, 5.0), (21.0, 5.0)],
            ],
            core: (19.4, 5.0),
            hint_en: "Everything at once. Final boss: Ransomware. Free the system.",
            hint_es: "Todo combinado. Jefe final: Ransomware. Libera el sistema.",
        },
    ]
}

// ----------------------------------------------------------- runtime structs
#[derive(Clone)]
struct Seg {
    a: (f32, f32),
    b: (f32, f32),
    len: f32,
}
#[derive(Clone)]
struct Metric {
    seg: Vec<Seg>,
    total: f32,
}

struct Enemy {
    kind: EKind,
    path: usize,
    dist: f32,
    hp: f32,
    max: f32,
    spd: f32,
    shield: f32,
    slow_until: f32,
    slow_factor: f32,
    flash: f32,
    elite: bool,
    aura_t: f32,
    wob: f32,   // Brownian perpendicular offset (tiles)
    wob_v: f32, // its velocity
    dead: bool,
    killed: bool,
    clone: bool, // spawned by a code gate (does not re-trigger gates, tiny reward)
}

struct Tower {
    kind: TKind,
    col: i32,
    row: i32,
    lvl: usize,
    cool: f32,
    recoil: f32,
    spent: i32,
    buff_dmg: f32,
    buff_range: f32,
    aim: f32,
}

enum Shot {
    Beam { x1: f32, y1: f32, x2: f32, y2: f32, t: f32, life: f32, c: Color },
    Bolt { x: f32, y: f32, target: usize, dmg: f32, crit: bool, spd: f32, c: Color },
    Mortar { x: f32, y: f32, tx: f32, ty: f32, t: f32, dur: f32, dmg: f32, splash: f32, c: Color },
}

struct Particle {
    ring: bool,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: f32,
    max: f32,
    t: f32,
    life: f32,
    sz: f32,
    c: Color,
}
struct FloatText {
    txt: String,
    x: f32,
    y: f32,
    t: f32,
    c: Color,
}

struct SpawnItem {
    t: f32,
    kind: EKind,
    hpm: f32,
    elite: bool,
    path: usize,
}

struct WaveGroup {
    kind: EKind,
    count: i32,
    gap: f32,
    start: f32,
    hpm: f32,
    elite_chance: f32,
}

// Collectible multiplier orb (good for the player). Drifts toward the Core;
// tap it before it's lost to grow your drone swarm.
struct Orb {
    path: usize,
    dist: f32,
    kind: OrbKind,
    bob: f32,
}

// Allied drone/satellite that orbits the Core and auto-fires at viruses.
struct Sat {
    ang: f32,
    rad: f32,   // orbit radius in tiles
    cool: f32,  // fire cooldown
    spd: f32,   // angular speed (signed: some orbit CW, some CCW)
    bob: f32,   // radial bob phase (the swarm breathes)
    flash: f32, // firing flash / lunge
    hue: f32,   // slight cyan->mint color variation
    dive: bool, // kamikaze run in progress
    px: f32,    // free position while diving
    py: f32,
    tx: f32, // dive target
    ty: f32,
}

// In-progress drag of a tower (drag & drop relocation).
struct Drag {
    ti: usize,
    sx: f32, // press origin
    sy: f32,
    x: f32, // current pointer
    y: f32,
    moved: bool, // exceeded the drag threshold (vs a plain tap)
}

struct Layout {
    tile: f32,
    ox: f32,
    oy: f32,
}

struct RainDrop {
    x: f32,
    y: f32,
    sp: f32,
    layer: u8,
    ch: char,
}

struct RadialOpt {
    x: f32,
    y: f32,
    icon: String,
    label: String,
    cost: String,
    c: Color,
    afford: bool,
    action: RadAction,
}
#[derive(Clone, Copy)]
enum RadAction {
    Build(TKind, i32, i32),
    Upgrade(usize),
    Sell(usize),
    Move(usize),
    Cancel,
}
struct Radial {
    cx: f32,
    cy: f32,
    opts: Vec<RadialOpt>,
}

struct Result {
    win: bool,
    title: String,
    msg: String,
    tag: String,
    accent: Color,
    reward: i32,
    last: bool,
}

// active level
struct Game {
    idx: usize,
    bytes: i32,
    lives: i32,
    max_lives: i32,
    towers: Vec<Tower>,
    enemies: Vec<Enemy>,
    shots: Vec<Shot>,
    parts: Vec<Particle>,
    texts: Vec<FloatText>,
    metrics: Vec<Metric>,
    path_cells: Vec<(i32, i32)>,
    orbs: Vec<Orb>,
    sats: Vec<Sat>,
    orb_timer: f32,  // time until the next FORK orb spawns
    kami_timer: f32, // time until the next random kamikaze run
    combo: i32,      // kill streak
    combo_t: f32,    // time left before the combo resets
    root: f32,      // "root access" energy for lane purges
    root_max: f32,
    anomalies: Vec<(i32, i32, bool)>, // col,row,is_buff
    waves: Vec<Vec<WaveGroup>>,
    wave_idx: usize,
    spawn_q: Vec<SpawnItem>,
    wave_on: bool,
    clock: f32,
    has_boss: bool,
    boss_hp: f32,
    boss_max: f32,
    won: bool,
    sel: Option<usize>,
    dim_shift_t: f32, // time until next dimension shift
    dim_active: f32,  // remaining duration of active shift
    next_timer: f32,  // countdown to auto-launch the next wave
    next_total: f32,  // its full duration (for the progress bar)
}

// ----------------------------------------------------------- App
struct App {
    maps: Vec<MapDef>,
    screen: Screen,
    paused: bool,
    lang_es: bool,
    crt: bool,
    fast: bool,
    unlocked: usize,
    game: Option<Game>,
    radial: Option<Radial>,
    result: Option<Result>,
    rain: Vec<RainDrop>,
    shake: f32,
    toast: String,
    toast_t: f32,
    last_w: f32,
    last_h: f32,
    dim_phase: f32,
    music_bed: Option<Sound>,
    music_drive: Option<Sound>,
    intensity: f32,
    sfx: HashMap<&'static str, Sound>,
    muted: bool,
    music_started: bool,
    moving: Option<usize>,
    drag: Option<Drag>,
    flash: f32,     // full-screen flash intensity (juice)
    flash_c: Color, // flash color
    slowmo: f32,    // brief slow-motion timer (real seconds)
}

const GLYPHS: &[char] = &[
    'ｱ', 'ｶ', 'ｻ', 'ﾀ', 'ﾅ', 'ﾊ', 'ﾏ', 'ﾔ', 'ﾗ', 'ﾜ', 'ﾝ', 'ｹ', 'ｼ', 'ｽ', '0', '1', '<', '>', '[',
    ']', '{', '}', '/', '\\', '=', '+', '*',
];

impl App {
    fn new() -> Self {
        App {
            maps: build_maps(),
            screen: Screen::Menu,
            paused: false,
            lang_es: false,
            crt: true,
            fast: false,
            unlocked: 1,
            game: None,
            radial: None,
            result: None,
            rain: Vec::new(),
            shake: 0.0,
            toast: String::new(),
            toast_t: 0.0,
            last_w: 0.0,
            last_h: 0.0,
            dim_phase: 0.0,
            music_bed: None,
            music_drive: None,
            intensity: 0.0,
            sfx: HashMap::new(),
            muted: false,
            music_started: false,
            moving: None,
            drag: None,
            flash: 0.0,
            flash_c: WHITE,
            slowmo: 0.0,
        }
    }

    fn set_flash(&mut self, c: Color, amt: f32) {
        if amt > self.flash {
            self.flash = amt;
        }
        self.flash_c = c;
    }
    // Decay screen-flash + slow-mo using REAL time (so slow-mo can't slow its own recovery).
    fn tick_fx(&mut self, real_dt: f32) {
        if self.flash > 0.0 {
            self.flash = (self.flash - real_dt * 2.6).max(0.0);
        }
        if self.slowmo > 0.0 {
            self.slowmo = (self.slowmo - real_dt).max(0.0);
        }
    }

    fn play_sfx(&self, name: &str) {
        if self.muted {
            return;
        }
        if let Some(s) = self.sfx.get(name) {
            play_sound(s, PlaySoundParams { looped: false, volume: 0.55 });
        }
    }
    fn start_music(&mut self) {
        if self.music_started || self.muted {
            return;
        }
        if let Some(m) = &self.music_bed {
            play_sound(m, PlaySoundParams { looped: true, volume: 0.4 });
        }
        if let Some(m) = &self.music_drive {
            play_sound(m, PlaySoundParams { looped: true, volume: 0.0 });
        }
        self.music_started = self.music_bed.is_some() || self.music_drive.is_some();
    }
    fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        if self.muted {
            if let Some(m) = &self.music_bed {
                stop_sound(m);
            }
            if let Some(m) = &self.music_drive {
                stop_sound(m);
            }
            self.music_started = false;
        } else {
            self.music_started = false;
            self.start_music();
        }
    }
    // Adaptive mix: the drive layer swells with on-screen threat during a wave.
    fn update_music_mix(&mut self, dt: f32) {
        if !self.music_started || self.muted {
            return;
        }
        let target = match &self.game {
            Some(g) if self.screen == Screen::Play && !self.paused && g.wave_on => {
                let e = g.enemies.len() as f32;
                let boss = if g.has_boss { 0.35 } else { 0.0 };
                (0.45 + (e * 0.025).min(0.45) + boss).min(1.0)
            }
            _ => 0.0,
        };
        self.intensity += (target - self.intensity) * (dt * 1.8).min(1.0);
        let i = self.intensity;
        if let Some(b) = &self.music_bed {
            set_sound_volume(b, 0.40 - i * 0.12);
        }
        if let Some(d) = &self.music_drive {
            set_sound_volume(d, i * 0.5);
        }
    }

    fn tr(&self, en: &str, es: &str) -> String {
        if self.lang_es { es.to_string() } else { en.to_string() }
    }

    fn rebuild_rain(&mut self) {
        let w = screen_width();
        let h = screen_height();
        self.rain.clear();
        let cols = (w / 16.0).floor().max(1.0) as i32;
        for i in 0..cols {
            for layer in 0..3u8 {
                let speed = match layer {
                    0 => gen_range(30.0, 70.0),
                    1 => gen_range(70.0, 130.0),
                    _ => gen_range(130.0, 220.0),
                };
                self.rain.push(RainDrop {
                    x: i as f32 * 16.0 + 4.0 + gen_range(-3.0, 3.0),
                    y: gen_range(0.0, h),
                    sp: speed,
                    layer,
                    ch: GLYPHS[gen_range(0, GLYPHS.len() as i32) as usize],
                });
            }
        }
    }

    // Narrow screens (phones in portrait) get the bigger-touch "mobile" UI.
    fn compact(&self) -> bool {
        screen_width() < 640.0
    }
    // Vertical space reserved for the HUD: top bar + bottom panel.
    fn hud_top(&self) -> f32 {
        if self.compact() { 50.0 } else { 44.0 }
    }
    fn hud_bottom(&self) -> f32 {
        if self.compact() { 122.0 } else { 64.0 }
    }

    fn layout(&self) -> Layout {
        let w = screen_width();
        let h = screen_height();
        let (cols, rows) = match &self.game {
            Some(g) => (self.maps[g.idx].cols as f32, self.maps[g.idx].rows as f32),
            None => (20.0, 11.0),
        };
        let pad = w.min(h) * 0.03;
        let top = self.hud_top();
        let bot = self.hud_bottom();
        let tile = ((w - pad * 2.0) / cols).min((h - pad * 2.0 - top - bot) / rows);
        let ox = (w - cols * tile) / 2.0;
        let oy = top + (h - top - bot - rows * tile) / 2.0;
        Layout { tile, ox, oy }
    }
    fn cc(&self, c: f32, r: f32) -> (f32, f32) {
        let l = self.layout();
        (l.ox + (c + 0.5) * l.tile, l.oy + (r + 0.5) * l.tile)
    }

    fn pos_at(&self, m: &Metric, dist: f32) -> (f32, f32, f32) {
        let l = self.layout();
        let mut d = dist;
        let n = m.seg.len();
        for (i, s) in m.seg.iter().enumerate() {
            if d <= s.len || i == n - 1 {
                let f = if s.len > 0.0 { d / s.len } else { 0.0 };
                let c = s.a.0 + (s.b.0 - s.a.0) * f;
                let r = s.a.1 + (s.b.1 - s.a.1) * f;
                let ang = (s.b.1 - s.a.1).atan2(s.b.0 - s.a.0);
                return (l.ox + (c + 0.5) * l.tile, l.oy + (r + 0.5) * l.tile, ang);
            }
            d -= s.len;
        }
        let last = &m.seg[n - 1];
        (l.ox + (last.b.0 + 0.5) * l.tile, l.oy + (last.b.1 + 0.5) * l.tile, 0.0)
    }

    // Position along the path, plus a Brownian perpendicular offset (in tiles).
    fn ewob(&self, m: &Metric, dist: f32, wob: f32) -> (f32, f32) {
        let p = self.pos_at(m, dist);
        let tile = self.layout().tile;
        let (px, py) = (-(p.2).sin(), (p.2).cos()); // unit perpendicular
        (p.0 + px * wob * tile, p.1 + py * wob * tile)
    }

    fn toast(&mut self, msg: String, secs: f32) {
        self.toast = msg;
        self.toast_t = secs;
    }

    // -------------------------------------------------- level setup
    fn start_level(&mut self, idx: usize) {
        let map = &self.maps[idx];
        // metrics
        let metrics: Vec<Metric> = map
            .paths
            .iter()
            .map(|p| {
                let mut seg = Vec::new();
                let mut total = 0.0;
                for i in 0..p.len() - 1 {
                    let a = p[i];
                    let b = p[i + 1];
                    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                    seg.push(Seg { a, b, len });
                    total += len;
                }
                Metric { seg, total }
            })
            .collect();
        // path cells
        let mut cells: Vec<(i32, i32)> = Vec::new();
        for p in &map.paths {
            for i in 0..p.len() - 1 {
                let a = p[i];
                let b = p[i + 1];
                let steps = (((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt() * 4.0).ceil() as i32;
                for s in 0..=steps {
                    let t = s as f32 / steps.max(1) as f32;
                    let c = (a.0 + (b.0 - a.0) * t).round() as i32;
                    let r = (a.1 + (b.1 - a.1) * t).round() as i32;
                    if !cells.contains(&(c, r)) {
                        cells.push((c, r));
                    }
                }
            }
        }
        // random anomaly tiles (buff / hazard) on a few buildable cells
        let mut anomalies: Vec<(i32, i32, bool)> = Vec::new();
        let n_anom = gen_range(2, 5);
        let mut tries = 0;
        while (anomalies.len() as i32) < n_anom && tries < 200 {
            tries += 1;
            let c = gen_range(0, map.cols);
            let r = gen_range(0, map.rows);
            if cells.contains(&(c, r)) {
                continue;
            }
            if (c, r) == (map.core.0 as i32, map.core.1 as i32) {
                continue;
            }
            if anomalies.iter().any(|a| a.0 == c && a.1 == r) {
                continue;
            }
            anomalies.push((c, r, gen_range(0.0, 1.0) > 0.4));
        }

        let waves = self.gen_waves(idx, map);

        let g = Game {
            idx,
            bytes: map.bytes,
            lives: map.lives,
            max_lives: map.lives,
            towers: Vec::new(),
            enemies: Vec::new(),
            shots: Vec::new(),
            parts: Vec::new(),
            texts: Vec::new(),
            metrics,
            path_cells: cells,
            orbs: Vec::new(),
            sats: Vec::new(),
            orb_timer: 4.0,
            kami_timer: 6.0,
            combo: 0,
            combo_t: 0.0,
            root: 100.0,
            root_max: 100.0,
            anomalies,
            waves,
            wave_idx: 0,
            spawn_q: Vec::new(),
            wave_on: false,
            clock: 0.0,
            has_boss: false,
            boss_hp: 0.0,
            boss_max: 0.0,
            won: false,
            sel: None,
            dim_shift_t: gen_range(14.0, 24.0),
            dim_active: 0.0,
            next_timer: 12.0,
            next_total: 12.0,
        };
        let hint = format!(
            "{} · {}",
            self.tr(map.hint_en, map.hint_es),
            self.tr("tap CYAN orbs to multiply your drones", "toca los orbes CYAN para multiplicar tus drones")
        );
        self.game = Some(g);
        // start with a small drone escort
        for k in 0..3 {
            self.add_sat(k);
        }
        self.screen = Screen::Play;
        self.paused = false;
        self.radial = None;
        self.result = None;
        self.moving = None;
        self.shake = 0.0;
        self.toast(hint, 4.2);
    }

    fn gen_waves(&self, lvl: usize, map: &MapDef) -> Vec<Vec<WaveGroup>> {
        let l = lvl as f32;
        let n = map.waves;
        let mut out = Vec::new();
        for w in 0..n {
            let wf = w as f32;
            let last = w == n - 1;
            // steeper than the original 1 + L*0.1 + w*0.05
            let hpm = 1.0 + l * 0.14 + wf * 0.08;
            let elite_chance = (0.04 + l * 0.02 + wf * 0.015).min(0.4);
            let mut g = Vec::new();
            // jittered base bits — more massive now (your drone swarm compensates)
            let base = (7.0 + wf * 2.2 + l * 1.8 + gen_range(-1.0, 3.0)).round() as i32;
            g.push(WaveGroup { kind: EKind::Bit, count: base.max(5), gap: 0.42, start: 0.0, hpm, elite_chance });
            if map.pools.contains(&EKind::Trojan) && w >= 1 {
                g.push(WaveGroup {
                    kind: EKind::Trojan,
                    count: 2 + w / 2 + (lvl as i32) / 2,
                    gap: 1.5,
                    start: 1.4,
                    hpm,
                    elite_chance,
                });
            }
            if map.pools.contains(&EKind::Worm) && w >= 2 && w % 2 == 0 {
                g.push(WaveGroup { kind: EKind::Worm, count: 3 + w / 2, gap: 0.85, start: 2.2, hpm, elite_chance });
            }
            if map.pools.contains(&EKind::Ddos) && w >= 2 && w % 3 == 1 {
                g.push(WaveGroup {
                    kind: EKind::Ddos,
                    count: 18 + w * 3 + gen_range(0, 6),
                    gap: 0.13,
                    start: 3.0,
                    hpm,
                    elite_chance,
                });
            }
            if map.pools.contains(&EKind::Corruptor) && w >= 3 && w % 3 == 0 {
                g.push(WaveGroup {
                    kind: EKind::Corruptor,
                    count: 1 + (lvl as i32) / 4,
                    gap: 2.2,
                    start: 2.6,
                    hpm,
                    elite_chance,
                });
            }
            // random surprise surge mid/late game
            if w >= 3 && gen_range(0.0, 1.0) > 0.55 {
                let extra = if map.pools.contains(&EKind::Ddos) { EKind::Ddos } else { EKind::Bit };
                g.push(WaveGroup { kind: extra, count: 10 + w * 2, gap: 0.16, start: 4.5, hpm, elite_chance });
            }
            if last && map.pools.contains(&EKind::Ransomware) {
                g.push(WaveGroup { kind: EKind::Ransomware, count: 1, gap: 1.0, start: 5.0, hpm: 1.0, elite_chance: 0.0 });
                g.push(WaveGroup { kind: EKind::Trojan, count: 4, gap: 1.2, start: 7.0, hpm, elite_chance });
                g.push(WaveGroup { kind: EKind::Corruptor, count: 2, gap: 2.0, start: 9.0, hpm, elite_chance });
            }
            out.push(g);
        }
        out
    }

    fn start_wave(&mut self) {
        let mut early_bonus = None;
        {
            let g = match &self.game {
                Some(g) => g,
                None => return,
            };
            if g.wave_idx >= g.waves.len() {
                return;
            }
            if g.wave_on && !g.spawn_q.is_empty() {
                return; // still spawning
            }
            if g.wave_on && g.spawn_q.is_empty() && !g.enemies.is_empty() {
                early_bonus = Some(15 + g.wave_idx as i32 * 3);
            }
        }
        let g = self.game.as_mut().unwrap();
        if let Some(b) = early_bonus {
            g.bytes += b;
        }
        let groups = &g.waves[g.wave_idx];
        let np = g.metrics.len();
        let mut q: Vec<SpawnItem> = Vec::new();
        for grp in groups {
            for i in 0..grp.count {
                let elite = gen_range(0.0, 1.0) < grp.elite_chance && grp.kind != EKind::Ransomware;
                let jitter = gen_range(-0.06, 0.06);
                q.push(SpawnItem {
                    t: (grp.start + i as f32 * grp.gap + jitter).max(0.0),
                    kind: grp.kind,
                    hpm: grp.hpm,
                    elite,
                    path: if np > 1 { gen_range(0, np as i32) as usize } else { 0 },
                });
            }
        }
        q.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        g.spawn_q = q;
        g.orb_timer = gen_range(2.5, 4.5); // first orb of the wave comes soon
        g.wave_on = true;
        g.clock = 0.0;
        g.wave_idx += 1;
        let wi = g.wave_idx;
        self.play_sfx("wave");
        self.set_flash(col(0x39, 0xff, 0x14), 0.3);
        self.toast(format!("{} - WAVE {}", self.tr("INCOMING", "ENTRANTE"), wi), 1.6);
    }

    fn spawn(&mut self, kind: EKind, hpm: f32, elite: bool, path: usize) {
        let g = self.game.as_mut().unwrap();
        let e = edef(kind);
        let mut hp = e.hp * hpm;
        let mut spd = e.spd;
        if elite {
            hp *= 2.2;
            spd *= 1.18;
        }
        if e.boss {
            g.has_boss = true;
            g.boss_hp = hp;
            g.boss_max = hp;
        }
        g.enemies.push(Enemy {
            kind,
            path,
            dist: -0.5 - gen_range(0.0, 0.3),
            hp,
            max: hp,
            spd,
            shield: 0.0,
            slow_until: 0.0,
            slow_factor: 0.0,
            flash: 0.0,
            elite,
            aura_t: 0.0,
            wob: 0.0,
            wob_v: gen_range(-1.0, 1.0),
            dead: false,
            killed: false,
            clone: false,
        });
    }

    // Add one allied drone to the orbiting swarm.
    fn add_sat(&mut self, seed: usize) {
        let g = self.game.as_mut().unwrap();
        let n = g.sats.len() as f32;
        let dir = if gen_range(0.0, 1.0) < 0.5 { 1.0 } else { -1.0 };
        g.sats.push(Sat {
            ang: (n + seed as f32) * 0.9,
            rad: 1.5 + gen_range(0.0, 1.9),
            cool: gen_range(0.0, 0.5),
            spd: dir * gen_range(0.7, 1.5),
            bob: gen_range(0.0, 6.28),
            flash: 0.0,
            hue: gen_range(0.0, 1.0),
            dive: false,
            px: 0.0,
            py: 0.0,
            tx: 0.0,
            ty: 0.0,
        });
    }

    // Collect a FORK orb: grow the drone swarm (capped) and remove the orb.
    fn collect_orb(&mut self, oi: usize) {
        let cap = 30usize;
        let (kind, path, dist) = {
            let g = self.game.as_ref().unwrap();
            let o = &g.orbs[oi];
            (o.kind, o.path, o.dist)
        };
        let (ox, oy) = {
            let g = self.game.as_ref().unwrap();
            let p = self.pos_at(&g.metrics[path], dist);
            (p.0, p.1)
        };
        let add = {
            let g = self.game.as_ref().unwrap();
            match kind {
                OrbKind::Double => g.sats.len().max(1),
                OrbKind::Add => 2,
            }
        };
        for k in 0..add {
            if self.game.as_ref().unwrap().sats.len() >= cap {
                break;
            }
            self.add_sat(k);
        }
        {
            let g = self.game.as_mut().unwrap();
            if oi < g.orbs.len() {
                g.orbs.remove(oi);
            }
        }
        self.sparks(ox, oy, col(0x9b, 0xf0, 0xff), 12);
        self.float(self.tr("+DRONES", "+DRONES"), ox, oy, col(0x9b, 0xf0, 0xff));
        self.play_sfx("upgrade");
    }

    // -------------------------------------------------- update
    fn update(&mut self, dt: f32) {
        // dimension phase always advances (visual)
        self.dim_phase += dt * 0.06;
        if self.toast_t > 0.0 {
            self.toast_t -= dt;
        }
        if self.shake > 0.0 {
            self.shake = (self.shake - dt * 60.0).max(0.0);
        }
        if self.game.is_none() {
            return;
        }

        // ---- dimension shift logic
        let mut dim_mul = 1.0f32;
        {
            let g = self.game.as_mut().unwrap();
            g.clock += dt;
            if g.dim_active > 0.0 {
                g.dim_active -= dt;
                dim_mul = 1.0;
            } else {
                g.dim_shift_t -= dt;
                if g.dim_shift_t <= 0.0 {
                    g.dim_active = gen_range(5.0, 8.0);
                    g.dim_shift_t = gen_range(16.0, 28.0);
                }
            }
        }
        let shifting = self.game.as_ref().unwrap().dim_active > 0.0;
        if shifting {
            dim_mul = 1.35; // virii surge during a dimension shift
        }

        // recharge ROOT energy + decay the kill-combo
        {
            let g = self.game.as_mut().unwrap();
            g.root = (g.root + dt * 16.0).min(g.root_max);
            if g.combo_t > 0.0 {
                g.combo_t -= dt;
                if g.combo_t <= 0.0 {
                    g.combo = 0;
                }
            }
        }

        // FORK orbs: spawn during waves, drift toward the Core, expire if uncollected
        {
            let spawn = {
                let g = self.game.as_mut().unwrap();
                if g.wave_on && !g.metrics.is_empty() {
                    g.orb_timer -= dt;
                    if g.orb_timer <= 0.0 {
                        g.orb_timer = gen_range(3.5, 6.5);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if spawn {
                let g = self.game.as_mut().unwrap();
                let np = g.metrics.len();
                let path = if np > 1 { gen_range(0, np as i32) as usize } else { 0 };
                let kind = if gen_range(0.0, 1.0) < 0.3 { OrbKind::Double } else { OrbKind::Add };
                g.orbs.push(Orb { path, dist: 0.5, kind, bob: gen_range(0.0, 6.28) });
            }
            let g = self.game.as_mut().unwrap();
            let totals: Vec<f32> = g.metrics.iter().map(|m| m.total).collect();
            for o in &mut g.orbs {
                o.dist += 1.1 * dt;
                o.bob += dt * 3.0;
            }
            g.orbs.retain(|o| o.dist < totals[o.path]);
        }

        // between-wave countdown: auto-launch when it runs out (click only advances it)
        {
            let arm = {
                let g = self.game.as_ref().unwrap();
                !g.wave_on && g.wave_idx < g.waves.len() && !g.won
            };
            if arm {
                let fire = {
                    let g = self.game.as_mut().unwrap();
                    g.next_timer -= dt;
                    g.next_timer <= 0.0
                };
                if fire {
                    self.start_wave();
                }
            }
        }

        // spawns
        loop {
            let due = {
                let g = self.game.as_ref().unwrap();
                g.spawn_q.first().map(|s| s.t <= g.clock).unwrap_or(false)
            };
            if !due {
                break;
            }
            let item = self.game.as_mut().unwrap().spawn_q.remove(0);
            self.spawn(item.kind, item.hpm, item.elite, item.path);
        }

        // enemies move + leaks + aura
        let l = self.layout();
        let positions: Vec<(f32, f32)> = {
            let g = self.game.as_ref().unwrap();
            g.enemies
                .iter()
                .map(|e| self.ewob(&g.metrics[e.path], e.dist, e.wob))
                .collect()
        };

        let mut life_loss = 0;
        let mut leak_fx: Vec<(f32, f32, bool)> = Vec::new();
        {
            let n = self.game.as_ref().unwrap().enemies.len();
            for i in 0..n {
                let (kind, dead) = {
                    let e = &self.game.as_ref().unwrap().enemies[i];
                    (e.kind, e.dead)
                };
                if dead {
                    continue;
                }
                let edf = edef(kind);
                let clock = self.game.as_ref().unwrap().clock;
                // move
                {
                    let g = self.game.as_mut().unwrap();
                    let e = &mut g.enemies[i];
                    let mut sp = e.spd * dim_mul;
                    if clock < e.slow_until {
                        sp *= 1.0 - e.slow_factor;
                    }
                    e.dist += sp * dt;
                    if e.flash > 0.0 {
                        e.flash -= dt;
                    }
                    // Brownian wander: damped random walk with spring-back to the path.
                    e.wob_v += gen_range(-7.0f32, 7.0f32) * dt;
                    e.wob_v -= e.wob * 3.0 * dt;
                    e.wob_v *= 1.0 - (2.0 * dt).min(0.9);
                    e.wob += e.wob_v * dt;
                    e.wob = e.wob.clamp(-0.65, 0.65);
                }
                let total = {
                    let g = self.game.as_ref().unwrap();
                    g.metrics[self.game.as_ref().unwrap().enemies[i].path].total
                };
                let dist = self.game.as_ref().unwrap().enemies[i].dist;
                if dist >= total {
                    let g = self.game.as_mut().unwrap();
                    g.enemies[i].dead = true;
                    life_loss += edf.leak;
                    let p = positions.get(i).cloned().unwrap_or((0.0, 0.0));
                    leak_fx.push((p.0, p.1, edf.boss));
                    continue;
                }
                // corruptor aura
                if edf.aura {
                    let mut do_pulse = false;
                    {
                        let g = self.game.as_mut().unwrap();
                        let e = &mut g.enemies[i];
                        e.aura_t -= dt;
                        if e.aura_t <= 0.0 {
                            e.aura_t = 2.2;
                            do_pulse = true;
                        }
                    }
                    if do_pulse {
                        let center = positions[i];
                        let nn = self.game.as_ref().unwrap().enemies.len();
                        for j in 0..nn {
                            if j == i {
                                continue;
                            }
                            let dead_j = self.game.as_ref().unwrap().enemies[j].dead;
                            if dead_j {
                                continue;
                            }
                            let pj = positions[j];
                            if ((pj.0 - center.0).powi(2) + (pj.1 - center.1).powi(2)).sqrt() < 2.2 * l.tile {
                                let g = self.game.as_mut().unwrap();
                                g.enemies[j].shield = (g.enemies[j].shield + 18.0).min(40.0);
                            }
                        }
                        let g = self.game.as_mut().unwrap();
                        g.enemies[i].shield = (g.enemies[i].shield + 18.0).min(40.0);
                    }
                }
            }
        }
        if !leak_fx.is_empty() {
            for (x, y, boss) in &leak_fx {
                self.boom(*x, *y, col(0x18, 0xe0, 0xff), 10);
                self.shake = self.shake.max(if *boss { 14.0 } else { 4.0 });
            }
            self.play_sfx("core");
            let dead = {
                let g = self.game.as_mut().unwrap();
                g.lives -= life_loss;
                if g.lives <= 0 {
                    g.lives = 0;
                    true
                } else {
                    false
                }
            };
            if dead {
                self.lose();
                return;
            }
        }

        // proxy buffs (compute then assign)
        {
            let ntw = self.game.as_ref().unwrap().towers.len();
            let mut buffs: Vec<(f32, f32)> = vec![(0.0, 0.0); ntw];
            for i in 0..ntw {
                let (ic, ir, _) = {
                    let t = &self.game.as_ref().unwrap().towers[i];
                    (t.col, t.row, t.kind)
                };
                let mut bd = 0.0;
                let mut br = 0.0;
                for j in 0..ntw {
                    if i == j {
                        continue;
                    }
                    let tj = &self.game.as_ref().unwrap().towers[j];
                    if tj.kind != TKind::Proxy {
                        continue;
                    }
                    let pd = tdef(TKind::Proxy);
                    let d = (((tj.col - ic).pow(2) + (tj.row - ir).pow(2)) as f32).sqrt();
                    if d <= pd.range[tj.lvl] {
                        bd += pd.buff_dmg[tj.lvl];
                        br += pd.buff_range[tj.lvl];
                    }
                }
                buffs[i] = (bd, br);
            }
            let g = self.game.as_mut().unwrap();
            for i in 0..ntw {
                g.towers[i].buff_dmg = buffs[i].0;
                g.towers[i].buff_range = buffs[i].1;
            }
        }

        // towers fire
        {
            let ntw = self.game.as_ref().unwrap().towers.len();
            for i in 0..ntw {
                // cool down
                {
                    let g = self.game.as_mut().unwrap();
                    g.towers[i].cool -= dt;
                    if g.towers[i].recoil > 0.0 {
                        g.towers[i].recoil -= dt;
                    }
                }
                let (kind, lvl, tc, tr_, bd, br, cool) = {
                    let t = &self.game.as_ref().unwrap().towers[i];
                    (t.kind, t.lvl, t.col, t.row, t.buff_dmg, t.buff_range, t.cool)
                };
                if kind == TKind::Proxy {
                    continue;
                }
                if cool > 0.0 {
                    continue;
                }
                let cfg = tdef(kind);
                let tp = self.cc(tc as f32, tr_ as f32);
                let range = cfg.range[lvl] * (1.0 + br) * l.tile;

                if cfg.proj == Proj::Frost {
                    // area pulse
                    let mut hit_any = false;
                    let nn = self.game.as_ref().unwrap().enemies.len();
                    let clock = self.game.as_ref().unwrap().clock;
                    for j in 0..nn {
                        let dead = self.game.as_ref().unwrap().enemies[j].dead;
                        if dead {
                            continue;
                        }
                        let pj = positions.get(j).cloned().unwrap_or((0.0, 0.0));
                        if ((pj.0 - tp.0).powi(2) + (pj.1 - tp.1).powi(2)).sqrt() <= range {
                            self.hurt(j, cfg.dmg[lvl] * (1.0 + bd), false);
                            let g = self.game.as_mut().unwrap();
                            g.enemies[j].slow_until = clock + cfg.slow_dur[lvl];
                            g.enemies[j].slow_factor = cfg.slow[lvl];
                            hit_any = true;
                        }
                    }
                    if hit_any {
                        {
                            let g = self.game.as_mut().unwrap();
                            g.towers[i].cool = cfg.cd[lvl];
                            g.parts.push(Particle {
                                ring: true, x: tp.0, y: tp.1, vx: 0.0, vy: 0.0, r: 4.0, max: range,
                                t: 0.4, life: 0.4, sz: 0.0, c: col(0x7f, 0xdc, 0xff),
                            });
                        }
                        self.play_sfx("frost");
                    }
                    continue;
                }

                // pick target: furthest along (or highest hp for antivirus)
                let mut best: Option<usize> = None;
                let mut best_score = -1.0f32;
                let nn = self.game.as_ref().unwrap().enemies.len();
                for j in 0..nn {
                    let (dead, hp, shield, ed) = {
                        let e = &self.game.as_ref().unwrap().enemies[j];
                        (e.dead, e.hp, e.shield, e.dist)
                    };
                    if dead {
                        continue;
                    }
                    let pj = positions.get(j).cloned().unwrap_or((0.0, 0.0));
                    let dd = ((pj.0 - tp.0).powi(2) + (pj.1 - tp.1).powi(2)).sqrt();
                    if dd <= range {
                        let score = if kind == TKind::Antivirus { hp + shield } else { ed };
                        if score > best_score {
                            best_score = score;
                            best = Some(j);
                        }
                    }
                }
                let bj = match best {
                    Some(j) => j,
                    None => continue,
                };
                let bp = positions[bj];
                let dmg_base = cfg.dmg[lvl] * (1.0 + bd);
                let crit = gen_range(0.0, 1.0) < 0.12;
                let dmg = if crit { dmg_base * 2.2 } else { dmg_base };
                {
                    let g = self.game.as_mut().unwrap();
                    g.towers[i].cool = cfg.cd[lvl];
                    g.towers[i].recoil = 0.12;
                    g.towers[i].aim = (bp.1 - tp.1).atan2(bp.0 - tp.0);
                }
                match cfg.proj {
                    Proj::Beam => {
                        {
                            let g = self.game.as_mut().unwrap();
                            g.shots.push(Shot::Beam { x1: tp.0, y1: tp.1, x2: bp.0, y2: bp.1, t: 0.1, life: 0.1, c: cfg.c2 });
                        }
                        self.hurt(bj, dmg, crit);
                        self.sparks(bp.0, bp.1, cfg.c2, 4);
                    }
                    Proj::Mortar => {
                        let g = self.game.as_mut().unwrap();
                        g.shots.push(Shot::Mortar {
                            x: tp.0, y: tp.1, tx: bp.0, ty: bp.1, t: 0.0, dur: 0.5, dmg,
                            splash: cfg.splash[lvl] * l.tile, c: cfg.c2,
                        });
                    }
                    _ => {
                        let g = self.game.as_mut().unwrap();
                        g.shots.push(Shot::Bolt { x: tp.0, y: tp.1, target: bj, dmg, crit, spd: 760.0, c: cfg.c2 });
                    }
                }
                self.play_sfx("shoot");
            }
        }

        // randomly send one orbiting drone on a kamikaze run into the densest swarm
        {
            let idx = self.game.as_ref().unwrap().idx;
            let fire = {
                let g = self.game.as_mut().unwrap();
                if g.wave_on && !g.enemies.is_empty() {
                    g.kami_timer -= dt;
                    if g.kami_timer <= 0.0 {
                        g.kami_timer = gen_range(4.0, 8.0);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if fire {
                let orbiting: Vec<usize> = {
                    let g = self.game.as_ref().unwrap();
                    (0..g.sats.len()).filter(|&i| !g.sats[i].dive).collect()
                };
                // keep at least 3 escorts orbiting before sacrificing one
                if orbiting.len() >= 4 {
                    if let Some((tx, ty)) = self.densest_enemy_pos() {
                        let pick = orbiting[gen_range(0, orbiting.len() as i32) as usize];
                        let core_t = self.maps[idx].core;
                        let (ccx, ccy) = self.cc(core_t.0, core_t.1);
                        let tile = l.tile;
                        {
                            let g = self.game.as_mut().unwrap();
                            let s = &mut g.sats[pick];
                            let rad = (s.rad + s.bob.sin() * 0.22) * tile;
                            s.px = ccx + s.ang.cos() * rad;
                            s.py = ccy + s.ang.sin() * rad;
                            s.tx = tx;
                            s.ty = ty;
                            s.dive = true;
                            s.flash = 0.0;
                        }
                        self.float(self.tr("KAMIKAZE", "KAMIKAZE"), tx, ty - 14.0, col(0xff, 0x95, 0x00));
                        self.play_sfx("wave");
                    }
                }
            }
        }

        // allied drones: orbit + auto-fire, or dive & detonate if on a kamikaze run
        {
            let core_t = self.maps[self.game.as_ref().unwrap().idx].core;
            let (ccx, ccy) = self.cc(core_t.0, core_t.1);
            let tile = l.tile;
            let idx = self.game.as_ref().unwrap().idx;
            let sat_dmg = 3.0 + idx as f32 * 1.3;
            let sat_range = 2.7 * tile;
            let kami_dmg = 42.0 + idx as f32 * 11.0;
            let kami_rad = 1.9 * tile;
            let ns = self.game.as_ref().unwrap().sats.len();
            let mut fires: Vec<(f32, f32, usize)> = Vec::new();
            let mut booms: Vec<(f32, f32)> = Vec::new();
            let mut consumed: Vec<usize> = Vec::new();
            for i in 0..ns {
                if self.game.as_ref().unwrap().sats[i].dive {
                    let arrived = {
                        let g = self.game.as_mut().unwrap();
                        let s = &mut g.sats[i];
                        s.bob += dt * 8.0;
                        let dx = s.tx - s.px;
                        let dy = s.ty - s.py;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let step = 540.0 * dt;
                        if dist <= step.max(9.0) {
                            s.px = s.tx;
                            s.py = s.ty;
                            true
                        } else {
                            s.px += dx / dist * step;
                            s.py += dy / dist * step;
                            false
                        }
                    };
                    if arrived {
                        let (bx, by) = {
                            let g = self.game.as_ref().unwrap();
                            (g.sats[i].px, g.sats[i].py)
                        };
                        booms.push((bx, by));
                        consumed.push(i);
                    }
                    continue;
                }
                let (sx, sy, can_fire) = {
                    let g = self.game.as_mut().unwrap();
                    let s = &mut g.sats[i];
                    s.ang += dt * s.spd;
                    s.bob += dt * 2.5;
                    s.cool -= dt;
                    if s.flash > 0.0 {
                        s.flash -= dt;
                    }
                    let rad = (s.rad + s.bob.sin() * 0.22) * tile;
                    let sx = ccx + s.ang.cos() * rad;
                    let sy = ccy + s.ang.sin() * rad;
                    (sx, sy, s.cool <= 0.0)
                };
                if !can_fire {
                    continue;
                }
                let mut best: Option<usize> = None;
                let mut bd = sat_range;
                let nn = self.game.as_ref().unwrap().enemies.len();
                for j in 0..nn {
                    let (dead, px, py) = {
                        let g = self.game.as_ref().unwrap();
                        let e = &g.enemies[j];
                        if e.dead {
                            (true, 0.0, 0.0)
                        } else {
                            let p = self.ewob(&g.metrics[e.path], e.dist, e.wob);
                            (false, p.0, p.1)
                        }
                    };
                    if dead {
                        continue;
                    }
                    let d = ((px - sx).powi(2) + (py - sy).powi(2)).sqrt();
                    if d < bd {
                        bd = d;
                        best = Some(j);
                    }
                }
                if let Some(j) = best {
                    {
                        let g = self.game.as_mut().unwrap();
                        g.sats[i].cool = 0.55;
                        g.sats[i].flash = 0.18;
                    }
                    fires.push((sx, sy, j));
                }
            }
            for (sx, sy, j) in fires {
                let tp = {
                    let g = self.game.as_ref().unwrap();
                    if j >= g.enemies.len() {
                        continue;
                    }
                    let e = &g.enemies[j];
                    self.ewob(&g.metrics[e.path], e.dist, e.wob)
                };
                {
                    let g = self.game.as_mut().unwrap();
                    g.shots.push(Shot::Beam { x1: sx, y1: sy, x2: tp.0, y2: tp.1, t: 0.08, life: 0.08, c: col(0x9b, 0xf0, 0xff) });
                }
                self.hurt(j, sat_dmg, false);
            }
            // kamikaze detonations: AoE damage at each impact point
            for (bx, by) in &booms {
                let nn = self.game.as_ref().unwrap().enemies.len();
                for j in 0..nn {
                    let (dead, px, py) = {
                        let g = self.game.as_ref().unwrap();
                        let e = &g.enemies[j];
                        if e.dead {
                            (true, 0.0, 0.0)
                        } else {
                            let p = self.ewob(&g.metrics[e.path], e.dist, e.wob);
                            (false, p.0, p.1)
                        }
                    };
                    if dead {
                        continue;
                    }
                    if ((px - bx).powi(2) + (py - by).powi(2)).sqrt() <= kami_rad {
                        self.hurt(j, kami_dmg, true);
                    }
                }
                self.boom(*bx, *by, col(0xff, 0x95, 0x00), 28);
                self.sparks(*bx, *by, col(0xff, 0xd8, 0x4d), 14);
                self.shake = self.shake.max(14.0);
                self.set_flash(col(0xff, 0x95, 0x00), 0.5);
                self.slowmo = self.slowmo.max(0.16);
                self.play_sfx("bigboom");
            }
            // remove the drones that sacrificed themselves
            if !consumed.is_empty() {
                consumed.sort_unstable();
                consumed.dedup();
                let g = self.game.as_mut().unwrap();
                for &ci in consumed.iter().rev() {
                    if ci < g.sats.len() {
                        g.sats.remove(ci);
                    }
                }
            }
        }

        // shots update
        {
            // recompute positions (enemies may have moved within this frame negligibly; reuse)
            let nsh = self.game.as_ref().unwrap().shots.len();
            let mut done = vec![false; nsh];
            for si in 0..nsh {
                enum Act {
                    None,
                    Hurt(usize, f32, bool, f32, f32, Color),
                    Mortar(f32, f32, f32, f32, Color),
                }
                let mut act = Act::None;
                {
                    let g = self.game.as_mut().unwrap();
                    match &mut g.shots[si] {
                        Shot::Beam { t, .. } => {
                            *t -= dt;
                            if *t <= 0.0 {
                                done[si] = true;
                            }
                        }
                        Shot::Bolt { x, y, target, dmg, crit, c, .. } => {
                            act = Act::Hurt(*target, *dmg, *crit, *x, *y, *c);
                        }
                        Shot::Mortar { tx, ty, t, dur, dmg, splash, c, .. } => {
                            *t += dt;
                            if *t >= *dur {
                                act = Act::Mortar(*tx, *ty, *dmg, *splash, *c);
                                done[si] = true;
                            }
                        }
                    }
                }
                match act {
                    Act::Hurt(tgt, dmg, crit, sx, sy, c) => {
                        // move bolt toward target, hit if close
                        let (dead, tp) = {
                            let g = self.game.as_ref().unwrap();
                            if tgt >= g.enemies.len() {
                                (true, (0.0, 0.0))
                            } else {
                                let e = &g.enemies[tgt];
                                let p = self.ewob(&g.metrics[e.path], e.dist, e.wob);
                                (e.dead, (p.0, p.1))
                            }
                        };
                        if dead {
                            done[si] = true;
                        } else {
                            let (nx, ny, hit) = {
                                let g = self.game.as_mut().unwrap();
                                if let Shot::Bolt { x, y, spd, .. } = &mut g.shots[si] {
                                    let a = (tp.1 - *y).atan2(tp.0 - *x);
                                    *x += a.cos() * *spd * dt;
                                    *y += a.sin() * *spd * dt;
                                    let hit = ((tp.0 - *x).powi(2) + (tp.1 - *y).powi(2)).sqrt() < 12.0;
                                    (*x, *y, hit)
                                } else {
                                    (sx, sy, false)
                                }
                            };
                            if hit {
                                self.hurt(tgt, dmg, crit);
                                self.sparks(nx, ny, c, 4);
                                done[si] = true;
                            }
                        }
                    }
                    Act::Mortar(tx, ty, dmg, splash, c) => {
                        self.boom(tx, ty, c, 16);
                        self.play_sfx("boom");
                        self.shake = self.shake.max(5.0);
                        let nn = self.game.as_ref().unwrap().enemies.len();
                        for j in 0..nn {
                            let (dead, p) = {
                                let g = self.game.as_ref().unwrap();
                                let e = &g.enemies[j];
                                let pp = self.ewob(&g.metrics[e.path], e.dist, e.wob);
                                (e.dead, (pp.0, pp.1))
                            };
                            if dead {
                                continue;
                            }
                            if ((p.0 - tx).powi(2) + (p.1 - ty).powi(2)).sqrt() <= splash {
                                self.hurt(j, dmg, false);
                            }
                        }
                    }
                    Act::None => {}
                }
            }
            let g = self.game.as_mut().unwrap();
            let mut idx = 0;
            g.shots.retain(|_| {
                let keep = !done[idx];
                idx += 1;
                keep
            });
        }

        // particles + texts
        {
            let g = self.game.as_mut().unwrap();
            for p in &mut g.parts {
                p.t -= dt;
                if !p.ring {
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    p.vy += 240.0 * dt;
                } else {
                    p.r += (p.max - p.r) * dt * 9.0;
                }
            }
            g.parts.retain(|p| p.t > 0.0);
            for tx in &mut g.texts {
                tx.t -= dt;
                tx.y -= 26.0 * dt;
            }
            g.texts.retain(|t| t.t > 0.0);
        }

        // handle kills (collect dead+killed)
        let kills: Vec<usize> = {
            let g = self.game.as_ref().unwrap();
            (0..g.enemies.len()).filter(|&i| g.enemies[i].dead && g.enemies[i].killed).collect()
        };
        for &i in &kills {
            self.on_kill(i);
        }
        // remove dead
        {
            let g = self.game.as_mut().unwrap();
            g.enemies.retain(|e| !e.dead);
            // boss tracking
            if g.has_boss {
                if let Some(b) = g.enemies.iter().find(|e| edef(e.kind).boss) {
                    g.boss_hp = b.hp;
                } else {
                    g.has_boss = false;
                }
            }
        }

        // wave clear
        let cleared = {
            let g = self.game.as_ref().unwrap();
            g.wave_on && g.spawn_q.is_empty() && g.enemies.is_empty()
        };
        if cleared {
            let (bonus, done_all) = {
                let g = self.game.as_mut().unwrap();
                g.wave_on = false;
                g.next_timer = 8.0;
                g.next_total = 8.0;
                let bonus = 25 + (g.wave_idx as i32 - 1) * 6 + g.idx as i32 * 5;
                g.bytes += bonus;
                (bonus, g.wave_idx >= g.waves.len())
            };
            let (cx, cy) = (screen_width() / 2.0, screen_height() * 0.4);
            self.float(format!("+{} BYTES", bonus), cx, cy, col(0xff, 0xd8, 0x4d));
            if done_all {
                self.win();
            } else {
                self.toast(format!("{} +{}", self.tr("WAVE CLEARED", "OLEADA LIMPIA"), bonus), 1.8);
            }
        }
    }

    fn hurt(&mut self, idx: usize, dmg: f32, crit: bool) {
        // read phase (immutable)
        let (valid, kind, path, dist0, wob) = {
            let g = self.game.as_ref().unwrap();
            if idx >= g.enemies.len() {
                (false, EKind::Bit, 0usize, 0.0, 0.0)
            } else {
                let e = &g.enemies[idx];
                (true, e.kind, e.path, e.dist, e.wob)
            }
        };
        if !valid {
            return;
        }
        let armor = edef(kind).armor;
        // position for the crit popup (two immutable borrows: self.game + self.ewob)
        let (ex, ey) = {
            let g = self.game.as_ref().unwrap();
            self.ewob(&g.metrics[path], dist0, wob)
        };
        // mutate (scoped)
        {
            let g = self.game.as_mut().unwrap();
            let e = &mut g.enemies[idx];
            let mut d = (dmg - armor).max(1.0);
            if e.shield > 0.0 {
                let a = e.shield.min(d);
                e.shield -= a;
                d -= a;
            }
            e.hp -= d;
            e.flash = 0.08;
            if e.hp <= 0.0 && !e.dead {
                e.dead = true;
                e.killed = true;
            }
        }
        if crit {
            self.float("CRIT".to_string(), ex, ey - 10.0, col(0xff, 0xf2, 0x6b));
        }
    }

    fn on_kill(&mut self, idx: usize) {
        let (kind, path, dist, elite, wob, is_clone) = {
            let g = self.game.as_ref().unwrap();
            let e = &g.enemies[idx];
            (e.kind, e.path, e.dist, e.elite, e.wob, e.clone)
        };
        let edf = edef(kind);
        let (px, py) = {
            let g = self.game.as_ref().unwrap();
            self.ewob(&g.metrics[path], dist, wob)
        };
        let reward = if is_clone { 1 } else { edf.reward + if elite { edf.reward } else { 0 } };
        // kill streak: each kill bumps the combo (and pays a small bonus)
        let combo = {
            let g = self.game.as_mut().unwrap();
            g.combo += 1;
            g.combo_t = 2.2;
            let cbonus = g.combo / 5;
            g.bytes += reward + cbonus;
            g.combo
        };
        self.float(format!("+{}", reward), px, py, col(0xff, 0xd8, 0x4d));
        // celebrate combo milestones
        if combo >= 5 && combo % 5 == 0 {
            let (sw, _) = (screen_width(), screen_height());
            self.float(format!("COMBO x{}", combo), sw / 2.0, 150.0, hsv(self.dim_phase * 2.0 + combo as f32 * 0.03, 0.9, 1.0));
            self.set_flash(hsv(self.dim_phase * 2.0, 0.8, 1.0), 0.22);
        }
        let kcol = if elite { hsv(self.dim_phase, 0.8, 1.0) } else { col(0x39, 0xff, 0x14) };
        let kn = if edf.draw > 1.0 { 12 } else { 7 };
        if edf.boss {
            self.boom(px, py, col(0xff, 0x95, 0x00), 34);
            self.shake = 24.0;
            self.set_flash(col(0xff, 0xd8, 0x4d), 0.75);
            self.slowmo = self.slowmo.max(0.28);
            self.play_sfx("bigboom");
        } else {
            self.boom(px, py, kcol, kn);
            self.play_sfx("boom");
        }
        // worm split
        if edf.split > 0 {
            let g = self.game.as_mut().unwrap();
            let bit = edef(EKind::Bit);
            for k in 0..edf.split {
                g.enemies.push(Enemy {
                    kind: EKind::Bit,
                    path,
                    dist: dist - k as f32 * 0.18,
                    hp: bit.hp,
                    max: bit.hp,
                    spd: bit.spd,
                    shield: 0.0,
                    slow_until: 0.0,
                    slow_factor: 0.0,
                    flash: 0.0,
                    elite: false,
                    aura_t: 0.0,
                    wob: gen_range(-0.3, 0.3),
                    wob_v: gen_range(-1.5, 1.5),
                    dead: false,
                    killed: false,
                    clone: true,
                });
            }
        }
    }

    fn sparks(&mut self, x: f32, y: f32, c: Color, n: i32) {
        let g = self.game.as_mut().unwrap();
        for _ in 0..n {
            let a = gen_range(0.0, std::f32::consts::TAU);
            let s = gen_range(40.0, 120.0);
            g.parts.push(Particle {
                ring: false, x, y, vx: a.cos() * s, vy: a.sin() * s, r: 0.0, max: 0.0,
                t: 0.3, life: 0.3, sz: gen_range(2.0, 4.0), c,
            });
        }
    }
    fn boom(&mut self, x: f32, y: f32, c: Color, n: i32) {
        let g = self.game.as_mut().unwrap();
        g.parts.push(Particle {
            ring: true, x, y, vx: 0.0, vy: 0.0, r: 3.0, max: 14.0 + n as f32, t: 0.35, life: 0.35, sz: 0.0, c,
        });
        for _ in 0..n {
            let a = gen_range(0.0, std::f32::consts::TAU);
            let s = gen_range(60.0, 200.0);
            g.parts.push(Particle {
                ring: false, x, y, vx: a.cos() * s, vy: a.sin() * s, r: 0.0, max: 0.0,
                t: gen_range(0.4, 0.7), life: 0.6, sz: gen_range(2.0, 5.0), c,
            });
        }
    }
    fn float(&mut self, txt: String, x: f32, y: f32, c: Color) {
        let g = self.game.as_mut().unwrap();
        g.texts.push(FloatText { txt, x, y, t: 1.0, c });
    }

    // -------------------------------------------------- build / sell
    fn build(&mut self, kind: TKind, c: i32, r: i32) {
        let cfg = tdef(kind);
        let (px, py) = self.cc(c as f32, r as f32);
        {
            let g = self.game.as_mut().unwrap();
            g.bytes -= cfg.cost[0];
            g.towers.push(Tower {
                kind, col: c, row: r, lvl: 0, cool: 0.0, recoil: 0.0, spent: cfg.cost[0],
                buff_dmg: 0.0, buff_range: 0.0, aim: 0.0,
            });
        }
        self.sparks(px, py, cfg.c2, 8);
        self.play_sfx("place");
        self.radial = None;
    }
    fn upgrade(&mut self, ti: usize) {
        let (kind, lvl, px, py) = {
            let g = self.game.as_ref().unwrap();
            let t = &g.towers[ti];
            let p = self.cc(t.col as f32, t.row as f32);
            (t.kind, t.lvl, p.0, p.1)
        };
        let cfg = tdef(kind);
        {
            let g = self.game.as_mut().unwrap();
            let cost = cfg.cost[lvl + 1];
            g.bytes -= cost;
            g.towers[ti].spent += cost;
            g.towers[ti].lvl += 1;
        }
        self.sparks(px, py, cfg.c2, 12);
        self.play_sfx("upgrade");
        self.radial = None;
    }
    fn sell(&mut self, ti: usize, refund: i32) {
        let (px, py) = {
            let g = self.game.as_ref().unwrap();
            let t = &g.towers[ti];
            self.cc(t.col as f32, t.row as f32)
        };
        {
            let g = self.game.as_mut().unwrap();
            g.bytes += refund;
            g.towers.remove(ti);
            g.sel = None;
        }
        self.sparks(px, py, col(0xff, 0x8b, 0x8b), 8);
        self.play_sfx("sell");
        self.radial = None;
    }

    fn win(&mut self) {
        {
            let g = self.game.as_ref().unwrap();
            if g.won {
                return;
            }
        }
        let (idx,) = {
            let g = self.game.as_mut().unwrap();
            g.won = true;
            (g.idx,)
        };
        let last = idx == self.maps.len() - 1;
        let reward = 60 + idx as i32 * 30;
        self.unlocked = self.unlocked.max((idx + 2).min(self.maps.len()));
        {
            let g = self.game.as_mut().unwrap();
            g.bytes += reward;
        }
        self.play_sfx("win");
        self.result = Some(Result {
            win: true,
            title: if last { self.tr("ALL SECTORS LIBERATED", "TODOS LOS SECTORES LIBERADOS") } else { self.tr("SYSTEM LIBERATED", "SISTEMA LIBERADO") },
            msg: if last { self.tr("You freed the entire server. The collective wins.", "Liberaste el servidor entero. El colectivo gana.") } else { self.tr("The Core holds. The data is free.", "El Nucleo resiste. La informacion es libre.") },
            tag: self.tr("// ACCESS GRANTED", "// ACCESO CONCEDIDO"),
            accent: col(0x39, 0xff, 0x14),
            reward,
            last,
        });
    }
    fn lose(&mut self) {
        self.play_sfx("lose");
        self.result = Some(Result {
            win: false,
            title: self.tr("SYSTEM CORRUPTED", "SISTEMA CORRUPTO"),
            msg: self.tr("The Core fell to corruption. Reboot and try again.", "El Nucleo cayo ante la corrupcion. Reinicia e intenta de nuevo."),
            tag: self.tr("// CONNECTION LOST", "// CONEXION PERDIDA"),
            accent: col(0xff, 0x5b, 0x4d),
            reward: 0,
            last: false,
        });
    }

    // Which lane is closest to a screen point, and how far along it (in tiles)?
    fn nearest_lane(&self, mx: f32, my: f32) -> Option<(usize, f32)> {
        let l = self.layout();
        let g = self.game.as_ref()?;
        let mut best: Option<(usize, f32, f32)> = None; // path, along, pixel_dist
        for (pi, m) in g.metrics.iter().enumerate() {
            let mut acc = 0.0;
            for s in &m.seg {
                let ax = l.ox + (s.a.0 + 0.5) * l.tile;
                let ay = l.oy + (s.a.1 + 0.5) * l.tile;
                let bx = l.ox + (s.b.0 + 0.5) * l.tile;
                let by = l.oy + (s.b.1 + 0.5) * l.tile;
                let dx = bx - ax;
                let dy = by - ay;
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 { (((mx - ax) * dx + (my - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let px = ax + dx * t;
                let py = ay + dy * t;
                let pd = ((mx - px).powi(2) + (my - py).powi(2)).sqrt();
                let along = acc + s.len * t;
                if best.map_or(true, |b| pd < b.2) {
                    best = Some((pi, along, pd));
                }
                acc += s.len;
            }
        }
        match best {
            Some((pi, along, pd)) if pd < l.tile * 1.3 => Some((pi, along)),
            _ => None,
        }
    }

    // "Center of the action": the live enemy with the most neighbors nearby.
    fn densest_enemy_pos(&self) -> Option<(f32, f32)> {
        let g = self.game.as_ref()?;
        if g.enemies.is_empty() {
            return None;
        }
        let tile = self.layout().tile;
        let r2 = (2.2 * tile).powi(2);
        let pos: Vec<(f32, f32)> = g
            .enemies
            .iter()
            .map(|e| {
                let p = self.ewob(&g.metrics[e.path], e.dist, e.wob);
                (p.0, p.1)
            })
            .collect();
        let mut best = None;
        let mut bestc = -1i32;
        for i in 0..pos.len() {
            if g.enemies[i].dead {
                continue;
            }
            let mut c = 0;
            for j in 0..pos.len() {
                if g.enemies[j].dead {
                    continue;
                }
                let d = (pos[i].0 - pos[j].0).powi(2) + (pos[i].1 - pos[j].1).powi(2);
                if d <= r2 {
                    c += 1;
                }
            }
            if c > bestc {
                bestc = c;
                best = Some(pos[i]);
            }
        }
        best
    }

    // Tower index sitting under a screen point, if any.
    fn tower_at(&self, mx: f32, my: f32) -> Option<usize> {
        let l = self.layout();
        let g = self.game.as_ref()?;
        let c = ((mx - l.ox) / l.tile).floor() as i32;
        let r = ((my - l.oy) / l.tile).floor() as i32;
        g.towers.iter().position(|t| t.col == c && t.row == r)
    }

    // Drop a dragged tower onto the node under the pointer (snaps; rejects invalid spots).
    fn drop_tower(&mut self, ti: usize, mx: f32, my: f32) {
        let l = self.layout();
        let (cols, rows, core) = {
            let g = self.game.as_ref().unwrap();
            let m = &self.maps[g.idx];
            (m.cols, m.rows, m.core)
        };
        let c = ((mx - l.ox) / l.tile).floor() as i32;
        let r = ((my - l.oy) / l.tile).floor() as i32;
        let in_bounds = c >= 0 && r >= 0 && c < cols && r < rows;
        let occupied = self
            .game
            .as_ref()
            .unwrap()
            .towers
            .iter()
            .enumerate()
            .any(|(j, t)| j != ti && t.col == c && t.row == r);
        let on_path = self.game.as_ref().unwrap().path_cells.contains(&(c, r));
        let is_core = (c, r) == (core.0 as i32, core.1 as i32);
        if in_bounds && !occupied && !on_path && !is_core {
            let (ox, oy) = {
                let t = &self.game.as_ref().unwrap().towers[ti];
                self.cc(t.col as f32, t.row as f32)
            };
            {
                let g = self.game.as_mut().unwrap();
                g.towers[ti].col = c;
                g.towers[ti].row = r;
            }
            let (nx, ny) = self.cc(c as f32, r as f32);
            self.sparks(ox, oy, col(0x18, 0xe0, 0xff), 6);
            self.sparks(nx, ny, col(0x18, 0xe0, 0xff), 8);
            self.play_sfx("place");
        } else {
            self.play_sfx("sell");
        }
        if let Some(g) = self.game.as_mut() {
            g.sel = None;
        }
    }

    // Lane purge ("kill -9"): spends ROOT energy to damage + slow every virus on a lane.
    fn lane_pulse(&mut self, path: usize, near: f32) {
        let cost = 28.0;
        if self.game.as_ref().unwrap().root < cost {
            self.toast(self.tr("ROOT LOW", "ROOT BAJO"), 1.0);
            self.play_sfx("sell");
            return;
        }
        let idx = self.game.as_ref().unwrap().idx;
        let dmg = 24.0 + idx as f32 * 7.0;
        {
            let g = self.game.as_mut().unwrap();
            g.root -= cost;
        }
        // damage + slow every live virus on this lane
        let (targets, clock) = {
            let g = self.game.as_ref().unwrap();
            let t: Vec<usize> = (0..g.enemies.len())
                .filter(|&j| !g.enemies[j].dead && g.enemies[j].path == path)
                .collect();
            (t, g.clock)
        };
        for &j in &targets {
            self.hurt(j, dmg, false);
            let g = self.game.as_mut().unwrap();
            if j < g.enemies.len() {
                g.enemies[j].slow_until = clock + 1.0;
                g.enemies[j].slow_factor = 0.4;
            }
        }
        // sweep of rings down the lane
        let pts: Vec<(f32, f32)> = {
            let g = self.game.as_ref().unwrap();
            let total = g.metrics[path].total;
            let mut v = Vec::new();
            let mut d = near;
            while d < total {
                let p = self.pos_at(&g.metrics[path], d);
                v.push((p.0, p.1));
                d += 0.85;
            }
            v
        };
        let tile = self.layout().tile;
        {
            let g = self.game.as_mut().unwrap();
            for (x, y) in pts {
                g.parts.push(Particle {
                    ring: true, x, y, vx: 0.0, vy: 0.0, r: 2.0, max: tile * 0.7,
                    t: 0.4, life: 0.4, sz: 0.0, c: col(0x2b, 0xff, 0x88),
                });
            }
        }
        self.shake = self.shake.max(4.0);
        self.play_sfx("frost");
    }

    // -------------------------------------------------- input
    fn handle_tap(&mut self, mx: f32, my: f32) {
        // overlays first
        if self.result.is_some() {
            return; // result buttons handled in draw_ui via separate hit test
        }
        if self.screen != Screen::Play || self.paused {
            return;
        }
        // radial open?
        if self.radial.is_some() {
            let hit_r = self.rad_r() + 2.0;
            let cancel_r = self.rad_r() * 0.78;
            let action = {
                let rad = self.radial.as_ref().unwrap();
                let mut chosen = RadAction::Cancel;
                let mut found = false;
                for o in &rad.opts {
                    if ((o.x - mx).powi(2) + (o.y - my).powi(2)).sqrt() <= hit_r {
                        chosen = o.action;
                        found = true;
                        break;
                    }
                }
                // center cancel
                if ((rad.cx - mx).powi(2) + (rad.cy - my).powi(2)).sqrt() <= cancel_r {
                    chosen = RadAction::Cancel;
                    found = true;
                }
                if found {
                    Some(chosen)
                } else {
                    Some(RadAction::Cancel) // tap outside closes
                }
            };
            match action {
                Some(RadAction::Build(k, c, r)) => {
                    let afford = self.game.as_ref().unwrap().bytes >= tdef(k).cost[0];
                    if afford {
                        self.build(k, c, r);
                    }
                }
                Some(RadAction::Upgrade(ti)) => {
                    let g = self.game.as_ref().unwrap();
                    let t = &g.towers[ti];
                    if t.lvl < 2 && g.bytes >= tdef(t.kind).cost[t.lvl + 1] {
                        self.upgrade(ti);
                    }
                }
                Some(RadAction::Sell(ti)) => {
                    let refund = (self.game.as_ref().unwrap().towers[ti].spent as f32 * 0.6).floor() as i32;
                    self.sell(ti, refund);
                }
                Some(RadAction::Move(ti)) => {
                    self.moving = Some(ti);
                    self.radial = None;
                    if let Some(g) = self.game.as_mut() {
                        g.sel = Some(ti);
                    }
                    self.play_sfx("place");
                    self.toast(self.tr("MOVE: tap a free node", "MOVER: toca un nodo libre"), 3.0);
                }
                _ => {
                    self.radial = None;
                    if let Some(g) = self.game.as_mut() {
                        g.sel = None;
                    }
                }
            }
            return;
        }
        // tap on board
        let l = self.layout();
        let (cols, rows, core) = {
            let g = self.game.as_ref().unwrap();
            let m = &self.maps[g.idx];
            (m.cols, m.rows, m.core)
        };
        let c = ((mx - l.ox) / l.tile).floor() as i32;
        let r = ((my - l.oy) / l.tile).floor() as i32;
        if c < 0 || r < 0 || c >= cols || r >= rows {
            return;
        }
        // relocating a tower? (MOVE chosen from the radial)
        if let Some(ti) = self.moving {
            let occupied = self
                .game
                .as_ref()
                .unwrap()
                .towers
                .iter()
                .enumerate()
                .any(|(j, t)| j != ti && t.col == c && t.row == r);
            let on_path = self.game.as_ref().unwrap().path_cells.contains(&(c, r));
            let is_core = (c, r) == (core.0 as i32, core.1 as i32);
            if !occupied && !on_path && !is_core {
                let (ox, oy) = {
                    let t = &self.game.as_ref().unwrap().towers[ti];
                    self.cc(t.col as f32, t.row as f32)
                };
                {
                    let g = self.game.as_mut().unwrap();
                    g.towers[ti].col = c;
                    g.towers[ti].row = r;
                }
                let (nx, ny) = self.cc(c as f32, r as f32);
                self.sparks(ox, oy, col(0x18, 0xe0, 0xff), 6);
                self.sparks(nx, ny, col(0x18, 0xe0, 0xff), 8);
                self.play_sfx("place");
            } else {
                self.play_sfx("sell");
            }
            self.moving = None;
            if let Some(g) = self.game.as_mut() {
                g.sel = None;
            }
            return;
        }
        // existing tower?
        let twi = {
            let g = self.game.as_ref().unwrap();
            g.towers.iter().position(|t| t.col == c && t.row == r)
        };
        if let Some(ti) = twi {
            self.game.as_mut().unwrap().sel = Some(ti);
            self.open_tower_radial(ti);
            return;
        }
        // path cell? -> collect a FORK orb if one is here, else purge the lane
        let on_path = self.game.as_ref().unwrap().path_cells.contains(&(c, r));
        if on_path {
            let hit_orb = {
                let g = self.game.as_ref().unwrap();
                let tile = self.layout().tile;
                let mut found = None;
                for (oi, o) in g.orbs.iter().enumerate() {
                    let p = self.pos_at(&g.metrics[o.path], o.dist);
                    if ((p.0 - mx).powi(2) + (p.1 - my).powi(2)).sqrt() < tile * 0.95 {
                        found = Some(oi);
                        break;
                    }
                }
                found
            };
            if let Some(oi) = hit_orb {
                self.collect_orb(oi);
            } else if let Some((pi, along)) = self.nearest_lane(mx, my) {
                self.lane_pulse(pi, along);
            }
            return;
        }
        if (c, r) == (core.0 as i32, core.1 as i32) {
            return;
        }
        self.open_build_radial(c, r);
    }

    // Radial option circle radius (bigger touch targets on mobile).
    fn rad_r(&self) -> f32 {
        if self.compact() { 40.0 } else { 31.0 }
    }
    fn radial_center(&self, c: i32, r: i32) -> (f32, f32, f32) {
        let p = self.cc(c as f32, r as f32);
        let rr = if self.compact() { 98.0 } else { 80.0 };
        let w = screen_width();
        let h = screen_height();
        let cx = p.0.clamp(rr + 36.0, w - rr - 36.0);
        let cy = p.1.clamp(rr + 60.0, h - rr - 96.0);
        (cx, cy, rr)
    }
    fn open_build_radial(&mut self, c: i32, r: i32) {
        let (cx, cy, rr) = self.radial_center(c, r);
        let towers = self.maps[self.game.as_ref().unwrap().idx].towers.clone();
        let bytes = self.game.as_ref().unwrap().bytes;
        let es = self.lang_es;
        let mut opts = Vec::new();
        let n = towers.len();
        for (i, k) in towers.iter().enumerate() {
            let cfg = tdef(*k);
            let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * (std::f32::consts::TAU / n as f32);
            let afford = bytes >= cfg.cost[0];
            opts.push(RadialOpt {
                x: cx + ang.cos() * rr,
                y: cy + ang.sin() * rr,
                icon: cfg.glyph.to_string(),
                label: if es { cfg.name_es.to_string() } else { cfg.name_en.to_string() },
                cost: format!("{}b", cfg.cost[0]),
                c: cfg.c1,
                afford,
                action: RadAction::Build(*k, c, r),
            });
        }
        self.radial = Some(Radial { cx, cy, opts });
    }
    fn open_tower_radial(&mut self, ti: usize) {
        let (kind, lvl, tc, tr_, spent) = {
            let t = &self.game.as_ref().unwrap().towers[ti];
            (t.kind, t.lvl, t.col, t.row, t.spent)
        };
        let bytes = self.game.as_ref().unwrap().bytes;
        let cfg = tdef(kind);
        let (cx, cy, rr) = self.radial_center(tc, tr_);
        let mut opts = Vec::new();
        let refund = (spent as f32 * 0.6).floor() as i32;
        if lvl < 2 {
            let upcost = cfg.cost[lvl + 1];
            opts.push(RadialOpt {
                x: cx, y: cy - rr, icon: "^".to_string(),
                label: self.tr("UPGRADE", "MEJORAR"), cost: format!("{}b", upcost),
                c: cfg.c1, afford: bytes >= upcost, action: RadAction::Upgrade(ti),
            });
        } else {
            opts.push(RadialOpt {
                x: cx, y: cy - rr, icon: "*".to_string(),
                label: self.tr("MAX", "MAX"), cost: "LVL3".to_string(),
                c: cfg.c1, afford: false, action: RadAction::Cancel,
            });
        }
        opts.push(RadialOpt {
            x: cx + rr * 0.92, y: cy + rr * 0.5, icon: "$".to_string(),
            label: self.tr("SELL", "VENDER"), cost: format!("+{}b", refund),
            c: col(0xc8, 0x1f, 0x3a), afford: true, action: RadAction::Sell(ti),
        });
        opts.push(RadialOpt {
            x: cx - rr * 0.92, y: cy + rr * 0.5, icon: "<>".to_string(),
            label: self.tr("MOVE", "MOVER"), cost: self.tr("FREE", "GRATIS"),
            c: col(0x18, 0xe0, 0xff), afford: true, action: RadAction::Move(ti),
        });
        self.radial = Some(Radial { cx, cy, opts });
    }

    // -------------------------------------------------- draw: background
    fn draw_rain(&mut self, dt: f32) {
        let w = screen_width();
        let h = screen_height();
        if (w - self.last_w).abs() > 1.0 || (h - self.last_h).abs() > 1.0 || self.rain.is_empty() {
            self.last_w = w;
            self.last_h = h;
            self.rebuild_rain();
        }
        // backdrop gradient-ish
        clear_background(col(0x02, 0x04, 0x0a));
        // dim rift hue (front layer is tinted toward it for the multi-dimensional feel)
        let rift = hsv(self.dim_phase, 0.7, 0.5);
        // parallax glyph layers
        for d in &mut self.rain {
            d.y += d.sp * dt;
            if d.y > h + 20.0 {
                d.y = -10.0;
                d.ch = GLYPHS[gen_range(0, GLYPHS.len() as i32) as usize];
                d.sp = match d.layer {
                    0 => gen_range(30.0, 70.0),
                    1 => gen_range(70.0, 130.0),
                    _ => gen_range(130.0, 220.0),
                };
            }
            let (size, base) = match d.layer {
                0 => (12.0, cola(26, 120, 60, 0.35)),
                1 => (16.0, cola(57, 200, 110, 0.6)),
                _ => (20.0, cola(180, 255, 200, 0.92)),
            };
            // tint front layer toward the current dimension hue
            let c = if d.layer == 2 { mix(base, with_a(rift, 0.9), 0.25) } else { base };
            let mut s = [0u8; 4];
            let st = d.ch.encode_utf8(&mut s);
            draw_text(st, d.x, d.y, size, c);
        }
    }

    // -------------------------------------------------- draw: board
    fn draw_board(&self) {
        let g = match &self.game {
            Some(g) => g,
            None => return,
        };
        let l = self.layout();
        let map = &self.maps[g.idx];
        let t = get_time() as f32;
        let shift = g.dim_active > 0.0;

        // shake
        let (sx, sy) = if self.shake > 0.0 {
            (gen_range(-self.shake, self.shake), gen_range(-self.shake, self.shake))
        } else {
            (0.0, 0.0)
        };
        let push = |x: f32, y: f32| (x + sx, y + sy);

        // build dots
        for r in 0..map.rows {
            for c in 0..map.cols {
                if g.path_cells.contains(&(c, r)) {
                    continue;
                }
                let (x, y) = self.cc(c as f32, r as f32);
                let (x, y) = push(x, y);
                let gc = cola(40, 120, 70, 0.16);
                draw_line(x - 2.5, y, x + 2.5, y, 1.0, gc);
                draw_line(x, y - 2.5, x, y + 2.5, 1.0, gc);
            }
        }
        // anomaly tiles
        for (c, r, buff) in &g.anomalies {
            let (x, y) = self.cc(*c as f32, *r as f32);
            let (x, y) = push(x, y);
            let pulse = 0.5 + (t * 3.0 + *c as f32).sin() * 0.3;
            let cc = if *buff { col(0x39, 0xff, 0x14) } else { col(0xff, 0x3b, 0x30) };
            draw_rectangle_lines(x - l.tile * 0.4, y - l.tile * 0.4, l.tile * 0.8, l.tile * 0.8, 2.0, with_a(cc, pulse));
            let sym = if *buff { "+" } else { "!" };
            draw_text(sym, x - 4.0, y + 5.0, 16.0, with_a(cc, pulse));
        }

        // paths
        let path_glow = if shift { hsv(self.dim_phase, 0.8, 1.0) } else { col(0x39, 0xff, 0x14) };
        for p in &map.paths {
            // base thick line
            for i in 0..p.len() - 1 {
                let a = self.cc(p[i].0, p[i].1);
                let b = self.cc(p[i + 1].0, p[i + 1].1);
                let (ax, ay) = push(a.0, a.1);
                let (bx, by) = push(b.0, b.1);
                draw_line(ax, ay, bx, by, l.tile * 0.82, cola(8, 40, 22, 0.95));
                draw_line(ax, ay, bx, by, l.tile * 0.62, cola(30, 156, 77, 0.5));
                draw_line(ax, ay, bx, by, 2.5, with_a(path_glow, 0.9));
            }
        }
        // via-nodes at each junction (circuit board feel)
        for p in &map.paths {
            for pt in p {
                let v = self.cc(pt.0, pt.1);
                let (vx, vy) = push(v.0, v.1);
                draw_rectangle(vx - 3.5, vy - 3.5, 7.0, 7.0, with_a(path_glow, 0.30));
                draw_rectangle_lines(vx - 3.5, vy - 3.5, 7.0, 7.0, 1.0, with_a(path_glow, 0.6));
            }
        }
        // flowing data packets
        for m in &g.metrics {
            let count = 6;
            for k in 0..count {
                let d = (t * 1.6 + k as f32 * m.total / count as f32) % m.total;
                let pp = self.pos_at(m, d);
                let (x, y) = push(pp.0, pp.1);
                draw_rectangle(x - 2.0, y - 2.0, 4.0, 4.0, with_a(path_glow, 0.8));
            }
        }
        // entry portals
        for m in &g.metrics {
            let p = self.pos_at(m, 0.0);
            let (x, y) = push(p.0, p.1);
            draw_circle(x, y, l.tile * 0.3 + (t * 4.0).sin() * 2.0, with_a(path_glow, 0.5));
        }

        // FORK orbs (collectible drone multipliers) drifting toward the Core
        for o in &g.orbs {
            let pp = self.pos_at(&g.metrics[o.path], o.dist);
            let (ox, oy) = push(pp.0, pp.1 + o.bob.sin() * 3.0);
            let cc = col(0x6b, 0xff, 0xe0); // bright cyan-mint = clearly good / yours
            let pulse = (0.6 + (t * 5.0 + o.dist).sin() * 0.3).clamp(0.3, 1.0);
            draw_circle_lines(ox, oy, l.tile * 0.52, 1.0, with_a(cc, pulse * 0.6));
            draw_poly(ox, oy, 4, l.tile * 0.34, t * 60.0 % 360.0, with_a(cc, 0.92));
            draw_poly_lines(ox, oy, 4, l.tile * 0.44, -t * 40.0 % 360.0, 2.0, with_a(WHITE, pulse));
            let lbl = match o.kind {
                OrbKind::Double => "x2",
                OrbKind::Add => "+",
            };
            let d = measure_text(lbl, None, 18, 1.0);
            draw_text(lbl, ox - d.width / 2.0, oy + 6.0, 18.0, col(0x02, 0x12, 0x10));
        }

        // core
        let core = self.cc(map.core.0, map.core.1);
        let (cx, cy) = push(core.0, core.1);
        let s = l.tile;
        let hs = s * 0.5;
        let pct = g.lives as f32 / g.max_lives as f32;
        let core_col = if pct > 0.4 { col(0x18, 0xe0, 0xff) } else { col(0xff, 0x5b, 0x4d) };
        let cyan = col(0x18, 0xe0, 0xff);
        // CPU pins (4 per side)
        let pin = s * 0.09;
        for k in 0..4 {
            let off = (k as f32 - 1.5) * s * 0.24;
            draw_rectangle(cx + off - pin * 0.5, cy - hs - pin, pin, pin, cyan);
            draw_rectangle(cx + off - pin * 0.5, cy + hs, pin, pin, cyan);
            draw_rectangle(cx - hs - pin, cy + off - pin * 0.5, pin, pin, cyan);
            draw_rectangle(cx + hs, cy + off - pin * 0.5, pin, pin, cyan);
        }
        // die plate + concentric ring
        draw_rectangle(cx - hs, cy - hs, s, s, col(0x06, 0x22, 0x2b));
        draw_rectangle_lines(cx - hs, cy - hs, s, s, 2.0, cyan);
        draw_rectangle_lines(cx - hs * 0.6, cy - hs * 0.6, hs * 1.2, hs * 1.2, 1.0, with_a(cyan, 0.4));
        // rotating core die
        let pulse = 1.0 + (t * 3.0).sin() * 0.08;
        draw_poly_lines(cx, cy, 4, s * 0.3, -t * 20.0, 1.0, with_a(core_col, 0.5));
        draw_poly(cx, cy, 4, s * 0.22 * pulse, 45.0 + t * 30.0, core_col);
        // lives bar (above the pins)
        draw_rectangle(cx - hs, cy - hs - pin - 7.0, s, 4.0, col(0x0a, 0x1f, 0x12));
        draw_rectangle(cx - hs, cy - hs - pin - 7.0, s * pct, 4.0, core_col);

        // allied drones orbiting the Core — neon ships with engine trails
        for sat in &g.sats {
            // kamikaze drone: hot, flickering, with an impact reticle
            if sat.dive {
                let (dx, dy) = push(sat.px, sat.py);
                let (rtx, rty) = push(sat.tx, sat.ty);
                let pr = (t * 16.0).sin().abs();
                let hot = mix(col(0xff, 0xd8, 0x4d), col(0xff, 0x3b, 0x30), pr);
                // target reticle
                let rr = l.tile * 0.6;
                draw_circle_lines(rtx, rty, rr, 1.5, with_a(col(0xff, 0x95, 0x00), 0.4 + 0.5 * pr));
                draw_line(rtx - rr, rty, rtx - rr * 0.5, rty, 1.5, with_a(col(0xff, 0x95, 0x00), 0.8));
                draw_line(rtx + rr * 0.5, rty, rtx + rr, rty, 1.5, with_a(col(0xff, 0x95, 0x00), 0.8));
                // streak trail behind the diving ship
                let ddx = sat.tx - sat.px;
                let ddy = sat.ty - sat.py;
                let dl = (ddx * ddx + ddy * ddy).sqrt().max(0.001);
                for k in 1..=5 {
                    let (sx2, sy2) = push(sat.px - ddx / dl * k as f32 * 6.0, sat.py - ddy / dl * k as f32 * 6.0);
                    draw_circle(sx2, sy2, (4.5 - k as f32 * 0.7).max(0.6), with_a(hot, 0.45 * (1.0 - k as f32 / 6.0)));
                }
                draw_circle(dx, dy, 9.0, with_a(hot, 0.35 + 0.25 * pr));
                draw_circle(dx, dy, 4.6, hot);
                draw_circle(dx, dy, 2.0, WHITE);
                continue;
            }
            let rad = (sat.rad + sat.bob.sin() * 0.22) * l.tile;
            let base_c = mix(col(0x18, 0xe0, 0xff), col(0x8b, 0xff, 0xd0), sat.hue);
            let dir = if sat.spd >= 0.0 { 1.0 } else { -1.0 };
            // engine trail: a short arc of fading dots behind the ship
            for k in 1..=4 {
                let a = sat.ang - dir * k as f32 * 0.10;
                let (tx, ty) = push(core.0 + a.cos() * rad, core.1 + a.sin() * rad);
                let al = 0.32 * (1.0 - k as f32 / 5.0);
                draw_circle(tx, ty, (3.4 - k as f32 * 0.55).max(0.6), with_a(base_c, al));
            }
            let (dx, dy) = push(core.0 + sat.ang.cos() * rad, core.1 + sat.ang.sin() * rad);
            let flash = (sat.flash / 0.18).clamp(0.0, 1.0);
            // soft glow (flares on fire)
            draw_circle(dx, dy, 7.0 + flash * 4.0, with_a(base_c, 0.16 + flash * 0.26));
            // ship triangle banked along its heading (tangent to the orbit)
            let head = sat.ang + dir * std::f32::consts::FRAC_PI_2;
            let sz = 5.2 + flash * 2.6;
            let (hx, hy) = (head.cos(), head.sin());
            let (rx, ry) = (-hy, hx);
            draw_triangle(
                Vec2::new(dx + hx * sz, dy + hy * sz),
                Vec2::new(dx - hx * sz * 0.7 + rx * sz * 0.7, dy - hy * sz * 0.7 + ry * sz * 0.7),
                Vec2::new(dx - hx * sz * 0.7 - rx * sz * 0.7, dy - hy * sz * 0.7 - ry * sz * 0.7),
                base_c,
            );
            // bright nucleus
            draw_circle(dx, dy, 1.8 + flash, WHITE);
        }

        // range preview for selected tower (follows the pointer while dragging)
        if let Some(ti) = g.sel {
            if ti < g.towers.len() {
                let tw = &g.towers[ti];
                let cfg = tdef(tw.kind);
                let dragging = self.drag.as_ref().map_or(false, |d| d.ti == ti && d.moved);
                let p = if dragging {
                    let d = self.drag.as_ref().unwrap();
                    (d.x, d.y)
                } else {
                    self.cc(tw.col as f32, tw.row as f32)
                };
                let (x, y) = push(p.0, p.1);
                let rad = cfg.range[tw.lvl] * (1.0 + tw.buff_range) * l.tile;
                draw_circle(x, y, rad, cola(57, 255, 20, 0.07));
                draw_circle_lines(x, y, rad, 1.5, cola(57, 255, 20, 0.4));
            }
        }

        // drag snap-target highlight (green = valid drop, red = blocked)
        if let Some(d) = &self.drag {
            if d.moved {
                let c = ((d.x - l.ox) / l.tile).floor() as i32;
                let r = ((d.y - l.oy) / l.tile).floor() as i32;
                let in_b = c >= 0 && r >= 0 && c < map.cols && r < map.rows;
                let occ = g.towers.iter().enumerate().any(|(j, tw)| j != d.ti && tw.col == c && tw.row == r);
                let onp = g.path_cells.contains(&(c, r));
                let isc = (c, r) == (map.core.0 as i32, map.core.1 as i32);
                let valid = in_b && !occ && !onp && !isc;
                let (hx, hy) = self.cc(c as f32, r as f32);
                let (hx, hy) = push(hx, hy);
                let cc = if valid { col(0x39, 0xff, 0x14) } else { col(0xff, 0x3b, 0x30) };
                draw_rectangle(hx - l.tile * 0.45, hy - l.tile * 0.45, l.tile * 0.9, l.tile * 0.9, with_a(cc, 0.12));
                draw_rectangle_lines(hx - l.tile * 0.45, hy - l.tile * 0.45, l.tile * 0.9, l.tile * 0.9, 2.0, with_a(cc, 0.8));
            }
        }

        // towers — "chip module" look: pinned circuit plate + rotating core + LEDs
        for (ti, tw) in g.towers.iter().enumerate() {
            let cfg = tdef(tw.kind);
            let dragging = self.drag.as_ref().map_or(false, |d| d.ti == ti && d.moved);
            let p = if dragging {
                let d = self.drag.as_ref().unwrap();
                (d.x, d.y)
            } else {
                self.cc(tw.col as f32, tw.row as f32)
            };
            let (x, y) = push(p.0, p.1);
            let sz = l.tile * 0.8 * (1.0 - tw.recoil * 3.0).max(0.7);
            let hs = sz * 0.46;
            // chip pins (3 per side)
            let pin = sz * 0.1;
            for k in 0..3 {
                let off = (k as f32 - 1.0) * sz * 0.26;
                draw_rectangle(x + off - pin * 0.5, y - hs - pin, pin, pin, cfg.c1);
                draw_rectangle(x + off - pin * 0.5, y + hs, pin, pin, cfg.c1);
                draw_rectangle(x - hs - pin, y + off - pin * 0.5, pin, pin, cfg.c1);
                draw_rectangle(x + hs, y + off - pin * 0.5, pin, pin, cfg.c1);
            }
            // plate + border
            draw_rectangle(x - hs, y - hs, hs * 2.0, hs * 2.0, cfg.c0);
            draw_rectangle_lines(x - hs, y - hs, hs * 2.0, hs * 2.0, 2.0, cfg.c1);
            // corner circuit brackets
            let q = hs * 0.66;
            let tc = with_a(cfg.c2, 0.45);
            draw_line(x - q, y - q, x - q + hs * 0.3, y - q, 1.0, tc);
            draw_line(x - q, y - q, x - q, y - q + hs * 0.3, 1.0, tc);
            draw_line(x + q, y + q, x + q - hs * 0.3, y + q, 1.0, tc);
            draw_line(x + q, y + q, x + q, y + q - hs * 0.3, 1.0, tc);
            // rotating core
            draw_poly(x, y, 6, sz * 0.27, t * 40.0 % 360.0, cfg.c1);
            draw_poly_lines(x, y, 6, sz * 0.27, -t * 30.0 % 360.0, 1.5, cfg.c2);
            // glyph (centered)
            let fs = sz * 0.46;
            let gd = measure_text(cfg.glyph, None, fs as u16, 1.0);
            draw_text(cfg.glyph, x - gd.width / 2.0, y + gd.height * 0.35, fs, cfg.c2);
            // level LEDs (top edge)
            for i in 0..=tw.lvl {
                draw_circle(x - hs + 5.0 + i as f32 * 7.0, y - hs - pin - 2.0, 2.0, cfg.c2);
            }
            // scanline sweep
            let sweep = (t * 1.3 + tw.col as f32 * 0.37).fract();
            let sy = y - hs + sweep * hs * 2.0;
            draw_line(x - hs, sy, x + hs, sy, 1.0, with_a(cfg.c2, 0.16));
            if tw.kind == TKind::Proxy {
                let a = 0.18 + (t * 3.0).sin() * 0.08;
                draw_circle_lines(x, y, cfg.range[tw.lvl] * l.tile, 1.0, with_a(cfg.c1, a));
            }
        }

        // enemies
        for e in &g.enemies {
            if e.dist < -0.3 {
                continue;
            }
            let edf = edef(e.kind);
            let p = self.ewob(&g.metrics[e.path], e.dist, e.wob);
            let (x, y) = push(p.0, p.1);
            let sz = edf.draw * l.tile;
            // aura ring
            if edf.aura {
                let a = 0.2 + (t * 4.0).sin() * 0.1;
                draw_circle_lines(x, y, 2.2 * l.tile, 1.5, with_a(col(0xff, 0x2b, 0xd6), a));
            }
            // body color: elites + boss hue-cycle
            let body = if e.flash > 0.0 {
                WHITE
            } else if edf.boss {
                hsv(self.dim_phase * 2.0, 0.8, 1.0)
            } else if e.elite {
                hsv(self.dim_phase + e.dist * 0.05, 0.85, 1.0)
            } else {
                edf.accent
            };
            let inner = if e.flash > 0.0 { WHITE } else { edf.base };
            let sides: u8 = if edf.boss { 8 } else { 4 };
            draw_poly(x, y, sides, sz * 0.5, t * 40.0 % 360.0, inner);
            draw_poly_lines(x, y, sides, sz * 0.5, t * 40.0 % 360.0, 2.0, body);
            draw_poly(x, y, sides, sz * 0.28, 0.0, body);
            if e.elite {
                draw_circle_lines(x, y, sz * 0.62, 1.5, hsv(self.dim_phase, 1.0, 1.0));
            }
            if e.shield > 0.0 {
                draw_circle_lines(x, y, sz * 0.62, 2.0, cola(127, 220, 255, 0.5));
            }
            if g.clock < e.slow_until {
                draw_rectangle(x - sz / 2.0, y - sz / 2.0, sz, sz, cola(127, 220, 255, 0.18));
            }
            // hp bar
            if e.hp < e.max {
                let bw = sz * 0.8;
                draw_rectangle(x - bw / 2.0, y - sz / 2.0 - 6.0, bw, 3.0, col(0x1a, 0x07, 0x07));
                let hc = if edf.boss { col(0xff, 0x3b, 0x30) } else { col(0x39, 0xff, 0x14) };
                draw_rectangle(x - bw / 2.0, y - sz / 2.0 - 6.0, bw * (e.hp / e.max).max(0.0), 3.0, hc);
            }
        }

        // shots
        for sh in &g.shots {
            match sh {
                Shot::Beam { x1, y1, x2, y2, t: tt, c, .. } => {
                    let (a0, b0) = push(*x1, *y1);
                    let (a1, b1) = push(*x2, *y2);
                    draw_line(a0, b0, a1, b1, 2.0 + *tt * 16.0, with_a(*c, (*tt * 10.0).min(1.0)));
                }
                Shot::Bolt { x, y, c, .. } => {
                    let (a, b) = push(*x, *y);
                    draw_rectangle(a - 2.5, b - 2.5, 5.0, 5.0, *c);
                }
                Shot::Mortar { x, y, tx, ty, t: tt, dur, c, .. } => {
                    let f = (*tt / *dur).clamp(0.0, 1.0);
                    let mx = x + (tx - x) * f;
                    let my = y + (ty - y) * f - (f * std::f32::consts::PI).sin() * l.tile * 1.6;
                    let (a, b) = push(mx, my);
                    draw_circle(a, b, 4.0, *c);
                }
            }
        }

        // particles
        for p in &g.parts {
            let a = (p.t / p.life).clamp(0.0, 1.0);
            let (x, y) = push(p.x, p.y);
            if p.ring {
                draw_circle_lines(x, y, p.r, 2.0, with_a(p.c, a * 0.8));
            } else {
                draw_rectangle(x - p.sz / 2.0, y - p.sz / 2.0, p.sz, p.sz, with_a(p.c, a));
            }
        }
        // floating texts
        for tx in &g.texts {
            let (x, y) = push(tx.x, tx.y);
            let a = (tx.t * 1.6).min(1.0);
            let d = measure_text(&tx.txt, None, 16, 1.0);
            draw_text(&tx.txt, x - d.width / 2.0, y, 16.0, with_a(tx.c, a));
        }

        // dimension-shift banner
        if shift {
            let txt = self.tr("// DIMENSION SHIFT", "// CAMBIO DIMENSIONAL");
            let d = measure_text(&txt, None, 22, 1.0);
            let hue = hsv(self.dim_phase * 3.0, 0.9, 1.0);
            draw_text(&txt, screen_width() / 2.0 - d.width / 2.0, 130.0, 22.0, hue);
        }
    }

    // -------------------------------------------------- UI helpers
    fn button(&self, taps: &[(f32, f32)], x: f32, y: f32, w: f32, h: f32, label: &str, fs: f32, fill: Color, text_c: Color) -> bool {
        let (mx, my) = mouse_position();
        let hover = mx >= x && mx <= x + w && my >= y && my <= y + h;
        let f = if hover { with_a(fill, (fill.a + 0.12).min(1.0)) } else { fill };
        draw_rectangle(x, y, w, h, f);
        draw_rectangle_lines(x, y, w, h, 1.5, with_a(text_c, 0.6));
        let d = measure_text(label, None, fs as u16, 1.0);
        draw_text(label, x + (w - d.width) / 2.0, y + (h + d.height) / 2.0 - 2.0, fs, text_c);
        taps.iter().any(|&(tx, ty)| tx >= x && tx <= x + w && ty >= y && ty <= y + h)
    }

    fn draw_ui(&mut self, taps: &[(f32, f32)]) {
        let w = screen_width();
        let h = screen_height();
        let green = col(0x39, 0xff, 0x14);
        let mint = col(0x7d, 0xff, 0xb0);
        let panel = cola(3, 14, 9, 0.78);
        let border = col(0x1c, 0x5a, 0x37);

        // global toggles top-right (shown when not playing or paused)
        let chrome = self.screen != Screen::Play || self.paused;
        if chrome {
            let compact = self.compact();
            let bwid = if compact { 54.0 } else { 40.0 };
            let bhei = if compact { 42.0 } else { 34.0 };
            let fsz = if compact { 15.0 } else { 13.0 };
            let step = bwid + 8.0;
            let mut bx = w - 12.0 - bwid;
            if self.button(taps, bx, 10.0, bwid, bhei, "CRT", fsz, panel, if self.crt { green } else { col(0x3f, 0x7a, 0x5a) }) {
                self.crt = !self.crt;
            }
            bx -= step;
            let lang = if self.lang_es { "ES" } else { "EN" };
            if self.button(taps, bx, 10.0, bwid, bhei, lang, fsz, panel, mint) {
                self.lang_es = !self.lang_es;
            }
            bx -= step;
            let snd_label = if self.muted { "OFF" } else { "SND" };
            let snd_col = if self.muted { col(0x3f, 0x7a, 0x5a) } else { green };
            if self.button(taps, bx, 10.0, bwid, bhei, snd_label, fsz, panel, snd_col) {
                self.toggle_mute();
            }
        }

        match self.screen {
            Screen::Menu => {
                let title = "KAMIKAZE HACKER";
                let fs = (w * 0.085).min(60.0);
                let d = measure_text(title, None, fs as u16, 1.0);
                let hue = hsv(self.dim_phase, 0.6, 1.0);
                let sub = "[ //ROOT_COLLECTIVE ]";
                let ds = measure_text(sub, None, 16, 1.0);
                draw_text(sub, w / 2.0 - ds.width / 2.0, h * 0.3, 16.0, col(0x18, 0xe0, 0xff));
                draw_text(title, w / 2.0 - d.width / 2.0, h * 0.42, fs, mix(green, hue, 0.3));
                let tag = self.tr("INFORMATION WANTS TO BE FREE", "LA INFORMACION QUIERE SER LIBRE");
                let dt2 = measure_text(&tag, None, 14, 1.0);
                draw_text(&tag, w / 2.0 - dt2.width / 2.0, h * 0.48, 14.0, mint);
                let bw = 240.0;
                if self.button(taps, w / 2.0 - bw / 2.0, h * 0.56, bw, 56.0, &format!("> {}", self.tr("JACK IN", "CONECTAR")), 18.0, col(0x1f, 0x9c, 0x4d), col(0x03, 0x10, 0x07)) {
                    self.screen = Screen::Select;
                }
                let note = self.tr("Tap a node to deploy hacker tools. Touch or mouse.", "Toca un nodo para desplegar herramientas. Tactil o raton.");
                let dn = measure_text(&note, None, 12, 1.0);
                draw_text(&note, w / 2.0 - dn.width / 2.0, h * 0.66, 12.0, col(0x2f, 0x60, 0x48));
                // discreet author credit
                let by = "Dr. Coronado x Claude";
                let db = measure_text(by, None, 12, 1.0);
                draw_text(by, w / 2.0 - db.width / 2.0, h * 0.92, 12.0, col(0x2a, 0x55, 0x40));
            }
            Screen::Select => {
                if self.button(taps, 16.0, 14.0, 90.0, 36.0, &format!("< {}", self.tr("BACK", "ATRAS")), 13.0, panel, mint) {
                    self.screen = Screen::Menu;
                }
                let title = self.tr("SELECT NODE", "ELIGE NODO");
                let dt2 = measure_text(&title, None, 18, 1.0);
                draw_text(&title, w / 2.0 - dt2.width / 2.0, 36.0, 18.0, green);
                // grid of map cards
                let cardw = 240.0;
                let cardh = 120.0;
                let gap = 14.0;
                let per_row = ((w - 32.0) / (cardw + gap)).floor().max(1.0) as usize;
                let total_w = per_row as f32 * (cardw + gap) - gap;
                let startx = (w - total_w) / 2.0;
                let starty = 70.0;
                let n = self.maps.len();
                let mut clicked: Option<usize> = None;
                for i in 0..n {
                    let r = i / per_row;
                    let cidx = i % per_row;
                    let x = startx + cidx as f32 * (cardw + gap);
                    let y = starty + r as f32 * (cardh + gap);
                    let locked = i >= self.unlocked;
                    let cleared = i + 1 < self.unlocked;
                    let bcol = if locked { cola(6, 12, 9, 0.6) } else { cola(5, 20, 12, 0.82) };
                    let bord = if locked { col(0x1a, 0x2c, 0x22) } else { col(0x1f, 0x6b, 0x40) };
                    draw_rectangle(x, y, cardw, cardh, bcol);
                    draw_rectangle_lines(x, y, cardw, cardh, 1.5, bord);
                    let name = if self.lang_es { self.maps[i].name_es } else { self.maps[i].name_en };
                    let numc = if locked { col(0x2f, 0x4a, 0x3a) } else { col(0x18, 0xe0, 0xff) };
                    let titc = if locked { col(0x3a, 0x5a, 0x48) } else { green };
                    draw_text(&format!("SECTOR {}", i + 1), x + 12.0, y + 24.0, 14.0, numc);
                    let status = if locked { self.tr("LOCKED", "BLOQUEADO") } else if cleared { "CLEAR".to_string() } else { ">".to_string() };
                    let sc = if locked { col(0x5a, 0x4a, 0x2a) } else if cleared { green } else { col(0xff, 0xd8, 0x4d) };
                    let ds = measure_text(&status, None, 12, 1.0);
                    draw_text(&status, x + cardw - 12.0 - ds.width, y + 24.0, 12.0, sc);
                    draw_text(name, x + 12.0, y + 56.0, 16.0, titc);
                    let stars = if cleared { "***" } else { "..." };
                    draw_text(&format!("WAVES {}  {}", self.maps[i].waves, stars), x + 12.0, y + 92.0, 12.0, col(0x2f, 0x60, 0x48));
                    if !locked && taps.iter().any(|&(tx, ty)| tx >= x && tx <= x + cardw && ty >= y && ty <= y + cardh) {
                        clicked = Some(i);
                    }
                }
                if let Some(i) = clicked {
                    self.start_level(i);
                }
            }
            Screen::Play => {
                self.draw_play_ui(taps, w, h, green, mint, panel, border);
            }
        }

        // toast
        if self.toast_t > 0.0 && !self.toast.is_empty() {
            let d = measure_text(&self.toast, None, 14, 1.0);
            let bx = w / 2.0 - d.width / 2.0 - 16.0;
            draw_rectangle(bx, 92.0, d.width + 32.0, 30.0, cola(3, 16, 10, 0.92));
            draw_rectangle_lines(bx, 92.0, d.width + 32.0, 30.0, 1.5, col(0x2b, 0xff, 0x88));
            draw_text(&self.toast, w / 2.0 - d.width / 2.0, 112.0, 14.0, col(0x2b, 0xff, 0x88));
        }

        // result overlay
        if self.result.is_some() {
            self.draw_result(taps, w, h);
        } else if self.screen == Screen::Play && self.paused {
            self.draw_pause(taps, w, h, green, mint, panel);
        }

        // radial menu
        if self.radial.is_some() {
            self.draw_radial();
        }

        // full-screen flash (juice on big hits / wave start / kamikaze)
        if self.flash > 0.0 {
            draw_rectangle(0.0, 0.0, w, h, with_a(self.flash_c, (self.flash * 0.6).min(0.6)));
        }

        // CRT overlay
        if self.crt {
            let mut y = 0.0;
            while y < h {
                draw_rectangle(0.0, y, w, 1.0, cola(0, 0, 0, 0.22));
                y += 3.0;
            }
            // vignette corners (cheap)
            draw_rectangle(0.0, 0.0, w, 6.0, cola(0, 0, 0, 0.3));
            draw_rectangle(0.0, h - 6.0, w, 6.0, cola(0, 0, 0, 0.3));
        }
    }

    fn draw_play_ui(&mut self, taps: &[(f32, f32)], w: f32, h: f32, green: Color, mint: Color, panel: Color, border: Color) {
        let g = self.game.as_ref().unwrap();
        let bytes = g.bytes;
        let lives = g.lives;
        let root = g.root;
        let root_max = g.root_max;
        let wave = g.wave_idx.min(g.waves.len());
        let waves = g.waves.len();
        let enemies = g.enemies.len();
        let boss_on = g.has_boss;
        let boss_pct = if g.boss_max > 0.0 { (g.boss_hp / g.boss_max).max(0.0) } else { 0.0 };
        let wave_on = g.wave_on;
        let spawn_left = g.spawn_q.len();
        let next_timer = g.next_timer;
        let next_total = g.next_total;
        let sats = g.sats.len();
        let combo = g.combo;
        let combo_t = g.combo_t;
        let name = if self.lang_es { self.maps[g.idx].name_es } else { self.maps[g.idx].name_en };

        let compact = self.compact();

        // top bar (map name + wave counter)
        let tb_w = if compact { (w - 150.0).clamp(140.0, 240.0) } else { 240.0 };
        draw_rectangle(10.0, 8.0, tb_w, 34.0, panel);
        draw_rectangle_lines(10.0, 8.0, tb_w, 34.0, 1.0, border);
        let nm_fs = if compact { 12.0 } else { 14.0 };
        draw_text(name, 18.0, 29.0, nm_fs, green);
        let wv = format!("W{}/{}", wave, waves);
        let dwv = measure_text(&wv, None, 12, 1.0);
        draw_text(&wv, 10.0 + tb_w - dwv.width - 8.0, 29.0, 12.0, col(0x3f, 0x7a, 0x5a));

        // kill-combo banner (top center, pulses and recolors as it climbs)
        if combo >= 3 {
            let txt = format!("COMBO x{}", combo);
            let scale = 1.0 + (combo as f32).min(30.0) * 0.018;
            let fs = (22.0 * scale).min(44.0);
            let hue = hsv(self.dim_phase * 2.0 + combo as f32 * 0.03, 0.85, 1.0);
            let d = measure_text(&txt, None, fs as u16, 1.0);
            let a = (combo_t / 2.2).clamp(0.35, 1.0);
            draw_text(&txt, w / 2.0 - d.width / 2.0, if compact { 44.0 } else { 46.0 }, fs, with_a(hue, a));
        }

        // top-right play toggles: fast + pause (bigger touch targets on mobile)
        let pf_w = if compact { 52.0 } else { 46.0 };
        let pf_h = if compact { 42.0 } else { 38.0 };
        let pf_fs = if compact { 18.0 } else { 14.0 };
        let flabel = if self.fast { "2x" } else { "1x" };
        if self.button(taps, w - 12.0 - pf_w, 8.0, pf_w, pf_h, "II", pf_fs, panel, mint) {
            self.paused = !self.paused;
        }
        if self.button(taps, w - 12.0 - pf_w * 2.0 - 8.0, 8.0, pf_w, pf_h, flabel, pf_fs, panel, if self.fast { green } else { mint }) {
            self.fast = !self.fast;
        }

        // boss bar
        if boss_on {
            let bw = (w * 0.6).min(520.0);
            let bx = w / 2.0 - bw / 2.0;
            let byo = if compact { 78.0 } else { 60.0 };
            draw_text("RANSOMWARE.EXE", bx, byo, 14.0, col(0xff, 0x5b, 0x4d));
            draw_rectangle(bx, byo + 6.0, bw, 12.0, col(0x1a, 0x07, 0x07));
            draw_rectangle(bx, byo + 6.0, bw * boss_pct, 12.0, col(0xff, 0x3b, 0x30));
            draw_rectangle_lines(bx, byo + 6.0, bw, 12.0, 1.0, col(0xff, 0x3b, 0x30));
        }

        // ---- bottom HUD ----
        let l_core = self.tr("CORE", "NUCLEO");
        let l_threat = self.tr("THREAT", "AMENAZA");
        let frac = (root / root_max).clamp(0.0, 1.0);
        let rcol = if frac >= 0.28 { col(0x2b, 0xff, 0x88) } else { col(0xff, 0x95, 0x00) };
        if compact {
            // mobile: a row of 5 evenly-spaced stat cells, big wave button below
            let area = self.hud_bottom();
            let y0 = h - area;
            draw_rectangle(0.0, y0, w, area, cola(2, 8, 5, 0.95));
            let m = 6.0;
            let gap = 5.0;
            let n = 5.0;
            let cw = (w - 2.0 * m - (n - 1.0) * gap) / n;
            let ch = 44.0;
            let cy = y0 + 6.0;
            let cell = |x: f32, label: &str, val: &str, vc: Color| {
                draw_rectangle(x, cy, cw, ch, cola(4, 16, 10, 0.85));
                draw_rectangle_lines(x, cy, cw, ch, 1.0, border);
                let dl = measure_text(label, None, 10, 1.0);
                draw_text(label, x + (cw - dl.width) / 2.0, cy + 15.0, 10.0, col(0x3f, 0x7a, 0x5a));
                let dv = measure_text(val, None, 18, 1.0);
                draw_text(val, x + (cw - dv.width) / 2.0, cy + 36.0, 18.0, vc);
            };
            let mut x = m;
            cell(x, "BYTES", &format!("{}", bytes), col(0xff, 0xd8, 0x4d));
            x += cw + gap;
            cell(x, &l_core, &format!("{}", lives), col(0x18, 0xe0, 0xff));
            x += cw + gap;
            cell(x, "DRONES", &format!("{}", sats), col(0x9b, 0xf0, 0xff));
            x += cw + gap;
            // ROOT cell with a bar instead of a number
            draw_rectangle(x, cy, cw, ch, cola(4, 16, 10, 0.85));
            draw_rectangle_lines(x, cy, cw, ch, 1.0, border);
            let dl = measure_text("ROOT", None, 10, 1.0);
            draw_text("ROOT", x + (cw - dl.width) / 2.0, cy + 15.0, 10.0, col(0x3f, 0x7a, 0x5a));
            draw_rectangle(x + 6.0, cy + 25.0, cw - 12.0, 9.0, col(0x0a, 0x1f, 0x12));
            draw_rectangle(x + 6.0, cy + 25.0, (cw - 12.0) * frac, 9.0, rcol);
            x += cw + gap;
            cell(x, &l_threat, &format!("{}", enemies), col(0xff, 0x5b, 0x4d));
        } else {
            // desktop: original layout
            draw_rectangle(0.0, h - 64.0, w, 64.0, cola(2, 8, 5, 0.92));
            draw_rectangle(10.0, h - 56.0, 96.0, 46.0, cola(4, 16, 10, 0.8));
            draw_rectangle_lines(10.0, h - 56.0, 96.0, 46.0, 1.0, border);
            draw_text("BYTES", 18.0, h - 38.0, 11.0, col(0x3f, 0x7a, 0x5a));
            draw_text(&format!("{}", bytes), 18.0, h - 18.0, 16.0, col(0xff, 0xd8, 0x4d));
            draw_rectangle(114.0, h - 56.0, 90.0, 46.0, cola(4, 16, 10, 0.8));
            draw_rectangle_lines(114.0, h - 56.0, 90.0, 46.0, 1.0, border);
            draw_text(&l_core, 122.0, h - 38.0, 11.0, col(0x3f, 0x7a, 0x5a));
            draw_text(&format!("{}", lives), 122.0, h - 18.0, 16.0, col(0x18, 0xe0, 0xff));
            draw_rectangle(212.0, h - 56.0, 96.0, 46.0, cola(4, 16, 10, 0.8));
            draw_rectangle_lines(212.0, h - 56.0, 96.0, 46.0, 1.0, border);
            draw_text("DRONES", 220.0, h - 38.0, 11.0, col(0x3f, 0x7a, 0x5a));
            draw_text(&format!("{}", sats), 220.0, h - 18.0, 16.0, col(0x9b, 0xf0, 0xff));
            draw_text(&l_threat, w - 90.0, h - 38.0, 11.0, col(0x3f, 0x7a, 0x5a));
            draw_text(&format!("{}", enemies), w - 90.0, h - 18.0, 16.0, col(0xff, 0x5b, 0x4d));
            let rbx = w - 232.0;
            draw_rectangle(rbx, h - 56.0, 120.0, 46.0, cola(4, 16, 10, 0.8));
            draw_rectangle_lines(rbx, h - 56.0, 120.0, 46.0, 1.0, border);
            draw_text("ROOT", rbx + 8.0, h - 38.0, 11.0, col(0x3f, 0x7a, 0x5a));
            draw_rectangle(rbx + 8.0, h - 26.0, 104.0, 8.0, col(0x0a, 0x1f, 0x12));
            draw_rectangle(rbx + 8.0, h - 26.0, 104.0 * frac, 8.0, rcol);
        }

        // start-wave button — with an auto-launch countdown bar
        let counting = !wave_on && wave < waves;
        let base = if wave > 0 {
            if wave_on && enemies > 0 && spawn_left == 0 {
                format!("! {}", self.tr("CALL EARLY", "LLAMAR YA"))
            } else {
                format!("> {}", self.tr("NEXT WAVE", "SIG. OLEADA"))
            }
        } else {
            format!("> {}", self.tr("START WAVE", "INICIAR OLEADA"))
        };
        let label = if counting {
            format!("{}  {}s", base, next_timer.max(0.0).ceil() as i32)
        } else {
            base
        };
        let disabled = wave_on && spawn_left > 0;
        let (bx, by, bw, bh, bfs) = if compact {
            (8.0, h - 58.0, w - 16.0, 50.0, 17.0)
        } else {
            (w / 2.0 - 100.0, h - 58.0, 200.0, 50.0, 14.0)
        };
        let fill = if disabled { cola(6, 22, 13, 0.8) } else { col(0x1f, 0x9c, 0x4d) };
        let tc = if disabled { col(0x3f, 0x7a, 0x5a) } else { col(0x03, 0x10, 0x07) };
        let clicked = self.button(taps, bx, by, bw, bh, &label, bfs, fill, tc);
        if counting && next_total > 0.0 {
            let cf = (1.0 - next_timer / next_total).clamp(0.0, 1.0);
            draw_rectangle(bx, by, bw * cf, bh, cola(57, 255, 20, 0.16));
            draw_rectangle(bx, by + bh - 3.0, bw, 3.0, cola(10, 40, 22, 0.9));
            draw_rectangle(bx, by + bh - 3.0, bw * cf, 3.0, col(0x39, 0xff, 0x14));
        }
        if clicked && !disabled {
            self.start_wave();
        }
    }

    fn draw_radial(&self) {
        let rad = self.radial.as_ref().unwrap();
        let r = self.rad_r();
        let cr = r * 0.78;
        let icon_fs = if self.compact() { 24.0 } else { 20.0 };
        // dim background
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), cola(0, 0, 0, 0.25));
        // center cancel
        draw_circle(rad.cx, rad.cy, cr, col(0x04, 0x11, 0x0b));
        draw_circle_lines(rad.cx, rad.cy, cr, 2.0, col(0x2b, 0x6b, 0x45));
        let dx = measure_text("X", None, icon_fs as u16, 1.0);
        draw_text("X", rad.cx - dx.width / 2.0, rad.cy + dx.height / 2.0, icon_fs, col(0x7d, 0xff, 0xb0));
        for o in &rad.opts {
            let fill = if o.afford { cola(6, 22, 13, 0.96) } else { cola(14, 10, 10, 0.92) };
            draw_circle(o.x, o.y, r, fill);
            draw_circle_lines(o.x, o.y, r, 2.0, if o.afford { o.c } else { col(0x5a, 0x4a, 0x4a) });
            let tc = if o.afford { o.c } else { col(0x5a, 0x4a, 0x4a) };
            let di = measure_text(&o.icon, None, icon_fs as u16, 1.0);
            draw_text(&o.icon, o.x - di.width / 2.0, o.y - r * 0.18, icon_fs, tc);
            let dl = measure_text(&o.label, None, 11, 1.0);
            draw_text(&o.label, o.x - dl.width / 2.0, o.y + r * 0.28, 11.0, tc);
            let dc = measure_text(&o.cost, None, 11, 1.0);
            draw_text(&o.cost, o.x - dc.width / 2.0, o.y + r * 0.62, 11.0, col(0xff, 0xd8, 0x4d));
        }
    }

    fn draw_pause(&mut self, taps: &[(f32, f32)], w: f32, h: f32, green: Color, mint: Color, panel: Color) {
        draw_rectangle(0.0, 0.0, w, h, cola(2, 6, 4, 0.82));
        let t = self.tr("PAUSED", "PAUSA");
        let d = measure_text(&t, None, 28, 1.0);
        draw_text(&t, w / 2.0 - d.width / 2.0, h * 0.32, 28.0, green);
        let bw = 220.0;
        let bx = w / 2.0 - bw / 2.0;
        if self.button(taps, bx, h * 0.4, bw, 50.0, &format!("> {}", self.tr("RESUME", "CONTINUAR")), 14.0, col(0x1f, 0x9c, 0x4d), col(0x03, 0x10, 0x07)) {
            self.paused = false;
        }
        if self.button(taps, bx, h * 0.4 + 60.0, bw, 46.0, &format!("@ {}", self.tr("RESTART", "REINICIAR")), 13.0, panel, mint) {
            let idx = self.game.as_ref().unwrap().idx;
            self.start_level(idx);
        }
        if self.button(taps, bx, h * 0.4 + 116.0, bw, 46.0, &format!("x {}", self.tr("ABORT", "ABORTAR")), 13.0, cola(20, 6, 5, 0.7), col(0xff, 0x8f, 0x86)) {
            self.quit_to_select();
        }
    }

    fn draw_result(&mut self, taps: &[(f32, f32)], w: f32, h: f32) {
        let (win, accent, last) = {
            let r = self.result.as_ref().unwrap();
            (r.win, r.accent, r.last)
        };
        draw_rectangle(0.0, 0.0, w, h, cola(2, 6, 4, 0.88));
        {
            let r = self.result.as_ref().unwrap();
            let dtag = measure_text(&r.tag, None, 14, 1.0);
            draw_text(&r.tag, w / 2.0 - dtag.width / 2.0, h * 0.28, 14.0, accent);
            let fs = (w * 0.06).min(40.0);
            let dt2 = measure_text(&r.title, None, fs as u16, 1.0);
            draw_text(&r.title, w / 2.0 - dt2.width / 2.0, h * 0.38, fs, accent);
            let dm = measure_text(&r.msg, None, 14, 1.0);
            draw_text(&r.msg, w / 2.0 - dm.width / 2.0, h * 0.46, 14.0, col(0x8e, 0xff, 0xc0));
            if r.reward > 0 {
                let rw = format!("+{} BYTES", r.reward);
                let dr = measure_text(&rw, None, 16, 1.0);
                draw_text(&rw, w / 2.0 - dr.width / 2.0, h * 0.52, 16.0, col(0xff, 0xd8, 0x4d));
            }
        }
        let compact = self.compact();
        if compact {
            // stack buttons vertically (full width) so they fit a phone
            let bw = (w - 48.0).min(320.0);
            let bx = w / 2.0 - bw / 2.0;
            let bh = 52.0;
            let gap = 12.0;
            let mut by = h * 0.58;
            if win {
                let nl = if last { self.tr("SECTORS", "SECTORES") } else { self.tr("NEXT SECTOR", "SIG. SECTOR") };
                if self.button(taps, bx, by, bw, bh, &format!("{} >", nl), 15.0, col(0x1f, 0x9c, 0x4d), col(0x03, 0x10, 0x07)) {
                    self.next_level();
                    return;
                }
                by += bh + gap;
            }
            if self.button(taps, bx, by, bw, bh, &format!("@ {}", self.tr("RETRY", "REINTENTAR")), 15.0, cola(4, 16, 10, 0.85), col(0x7d, 0xff, 0xb0)) {
                let idx = self.game.as_ref().unwrap().idx;
                self.result = None;
                self.start_level(idx);
                return;
            }
            by += bh + gap;
            if self.button(taps, bx, by, bw, bh, &format!("# {}", self.tr("SECTORS", "SECTORES")), 15.0, cola(4, 16, 10, 0.85), col(0x7d, 0xff, 0xb0)) {
                self.quit_to_select();
            }
            return;
        }
        let bw = 180.0;
        let total = if win { 3.0 } else { 2.0 };
        let gap = 12.0;
        let tw = total * bw + (total - 1.0) * gap;
        let mut bx = w / 2.0 - tw / 2.0;
        let by = h * 0.6;
        if win {
            let nl = if last { self.tr("SECTORS", "SECTORES") } else { self.tr("NEXT SECTOR", "SIG. SECTOR") };
            if self.button(taps, bx, by, bw, 50.0, &format!("{} >", nl), 13.0, col(0x1f, 0x9c, 0x4d), col(0x03, 0x10, 0x07)) {
                self.next_level();
                return;
            }
            bx += bw + gap;
        }
        if self.button(taps, bx, by, bw, 50.0, &format!("@ {}", self.tr("RETRY", "REINTENTAR")), 13.0, cola(4, 16, 10, 0.85), col(0x7d, 0xff, 0xb0)) {
            let idx = self.game.as_ref().unwrap().idx;
            self.result = None;
            self.start_level(idx);
            return;
        }
        bx += bw + gap;
        if self.button(taps, bx, by, bw, 50.0, &format!("# {}", self.tr("SECTORS", "SECTORES")), 13.0, cola(4, 16, 10, 0.85), col(0x7d, 0xff, 0xb0)) {
            self.quit_to_select();
        }
    }

    fn next_level(&mut self) {
        let last = self.result.as_ref().map(|r| r.last).unwrap_or(false);
        self.result = None;
        if last {
            self.quit_to_select();
            return;
        }
        let n = self.game.as_ref().unwrap().idx + 1;
        if n < self.maps.len() {
            self.start_level(n);
        } else {
            self.quit_to_select();
        }
    }
    fn quit_to_select(&mut self) {
        self.game = None;
        self.screen = Screen::Select;
        self.paused = false;
        self.result = None;
        self.radial = None;
        self.moving = None;
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    macroquad::rand::srand(0x5151_2526);
    let mut app = App::new();

    // Synthesize and load all audio (in-memory WAV, no external files).
    app.music_bed = load_sound_from_bytes(&build_bed()).await.ok();
    app.music_drive = load_sound_from_bytes(&build_drive()).await.ok();
    for &name in SFX_NAMES {
        if let Ok(s) = load_sound_from_bytes(&build_sfx(name)).await {
            app.sfx.insert(name, s);
        }
    }

    loop {
        let dt = get_frame_time().min(0.05);
        app.tick_fx(dt); // decay flash + slow-mo in real time
        let sim_dt = if app.slowmo > 0.0 { dt * 0.35 } else { dt };

        // gather taps (mouse + touch). Prefer touch; only use mouse when there are no touches
        // this frame, so emulated mouse events on mobile don't fire a second tap.
        let mut taps: Vec<(f32, f32)> = Vec::new();
        let active_touches = touches();
        let mut had_touch = false;
        for t in &active_touches {
            if t.phase == TouchPhase::Started {
                taps.push((t.position.x, t.position.y));
            }
            had_touch = true;
        }
        if !had_touch && is_mouse_button_pressed(MouseButton::Left) {
            taps.push(mouse_position());
        }

        // Unified pointer (touch-first) for dragging towers: position + held/released state.
        let mut ptr_pos = mouse_position();
        let mut ptr_down = is_mouse_button_down(MouseButton::Left);
        let mut ptr_released = is_mouse_button_released(MouseButton::Left);
        if had_touch {
            if let Some(t) = active_touches.first() {
                ptr_pos = (t.position.x, t.position.y);
                ptr_down = matches!(t.phase, TouchPhase::Started | TouchPhase::Moved | TouchPhase::Stationary);
                ptr_released = matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled);
            }
        }


        // ---- tower drag & drop ----
        let can_drag = app.screen == Screen::Play
            && app.result.is_none()
            && !app.paused
            && app.radial.is_none()
            && app.moving.is_none();
        if can_drag && app.drag.is_none() {
            if let Some(&(tx, ty)) = taps.first() {
                if let Some(ti) = app.tower_at(tx, ty) {
                    if let Some(g) = app.game.as_mut() {
                        g.sel = Some(ti);
                    }
                    app.drag = Some(Drag { ti, sx: tx, sy: ty, x: tx, y: ty, moved: false });
                    taps.clear(); // this press becomes a drag, not a build/radial tap
                    app.start_music();
                }
            }
        }
        if app.drag.is_some() {
            {
                let d = app.drag.as_mut().unwrap();
                d.x = ptr_pos.0;
                d.y = ptr_pos.1;
                if ((d.x - d.sx).powi(2) + (d.y - d.sy).powi(2)).sqrt() > 8.0 {
                    d.moved = true;
                }
            }
            if ptr_released || !ptr_down {
                let d = app.drag.take().unwrap();
                if d.moved {
                    app.drop_tower(d.ti, ptr_pos.0, ptr_pos.1);
                } else {
                    // a plain tap (no drag) -> open the tower's radial menu
                    app.open_tower_radial(d.ti);
                }
            }
        }

        // Browsers block audio until a user gesture - start music on the first tap.
        if !taps.is_empty() {
            app.start_music();
        }
        app.update_music_mix(dt);

        // background rain (always)
        app.draw_rain(dt);

        // simulate
        if app.screen == Screen::Play && !app.paused && app.result.is_none() {
            let steps = if app.fast { 2 } else { 1 };
            for _ in 0..steps {
                app.update(sim_dt);
                if app.result.is_some() {
                    break;
                }
            }
        } else {
            // still advance dim phase / toast timers off-board
            app.dim_phase += dt * 0.06;
            if app.toast_t > 0.0 {
                app.toast_t -= dt;
            }
            if app.shake > 0.0 {
                app.shake = (app.shake - dt * 60.0).max(0.0);
            }
        }

        // draw board
        if app.screen == Screen::Play {
            app.draw_board();
        }

        // handle taps that hit the board (build/sell/radial) — only when no overlay buttons consumed.
        // We process board taps AFTER UI so UI buttons (handled in draw_ui via taps) take priority;
        // to keep it simple, board taps are processed here using the same tap list. UI hit-tests are
        // idempotent (a tap on a button also triggers the button), so we guard board taps to the
        // playfield region and only when no overlay is active.
        let overlay = app.result.is_some() || app.paused;
        // draw + handle UI (buttons consume via internal hit-test)
        app.draw_ui(&taps);

        if app.screen == Screen::Play && !overlay {
            let top_strip = app.hud_top();
            let bot_strip = screen_height() - app.hud_bottom();
            for &(tx, ty) in &taps {
                // ignore taps in the HUD strips so buttons don't double-fire builds
                if ty < top_strip || ty > bot_strip {
                    // still allow radial interactions which may be near edges
                    if app.radial.is_some() {
                        app.handle_tap(tx, ty);
                    }
                    continue;
                }
                app.handle_tap(tx, ty);
            }
        } else if app.radial.is_some() {
            for &(tx, ty) in &taps {
                app.handle_tap(tx, ty);
            }
        }

        next_frame().await;
    }
}
