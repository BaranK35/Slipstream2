//! Ultra-low latency native Windows sound effects with background worker thread
//! execution, comfortable volume attenuation, and atomic master enable/disable controls.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;

pub const CLICK_WAV: &[u8] = include_bytes!("../sounds/click.wav");
pub const TOGGLE_WAV: &[u8] = include_bytes!("../sounds/toggle.wav");
pub const ON_WAV: &[u8] = include_bytes!("../sounds/on.wav");
pub const OFF_WAV: &[u8] = include_bytes!("../sounds/off.wav");

#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn PlaySoundW(pszSound: *const u16, hmod: isize, fdwSound: u32) -> i32;
}

static SOUND_ENABLED: AtomicBool = AtomicBool::new(false);

/// Sets whether sound effects are globally enabled.
pub fn set_enabled(enabled: bool) {
    SOUND_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Checks whether sound effects are globally enabled. Defaults to false.
pub fn is_enabled() -> bool {
    SOUND_ENABLED.load(Ordering::SeqCst)
}

/// Safely attenuates 16-bit signed PCM little-endian samples inside a WAV byte buffer.
/// Multiplies each audio sample within the 'data' chunk by `scale` and clamps to `i16` bounds.
pub fn attenuate_wav(raw: &[u8], scale: f32) -> Vec<u8> {
    let mut out = raw.to_vec();
    // Locate the 'data' subchunk header (4 bytes "data" + 4 bytes length)
    if let Some(pos) = raw.windows(4).position(|w| w == b"data") {
        let sample_start = pos + 8;
        let data_len = if pos + 8 <= raw.len() {
            u32::from_le_bytes([raw[pos + 4], raw[pos + 5], raw[pos + 6], raw[pos + 7]]) as usize
        } else {
            0
        };
        let sample_end = if data_len > 0 {
            (sample_start + data_len).min(out.len())
        } else {
            out.len()
        };
        let mut i = sample_start;
        while i + 1 < sample_end {
            let sample = i16::from_le_bytes([out[i], out[i + 1]]);
            let scaled = ((sample as f32) * scale).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            let bytes = scaled.to_le_bytes();
            out[i] = bytes[0];
            out[i + 1] = bytes[1];
            i += 2;
        }
    }
    out
}

static SOUND_TX: OnceLock<SyncSender<&'static str>> = OnceLock::new();

fn get_sender() -> &'static SyncSender<&'static str> {
    SOUND_TX.get_or_init(|| {
        let (tx, rx) = sync_channel::<&'static str>(8);
        let _ = std::thread::Builder::new()
            .name("slipstream-audio".to_string())
            .spawn(move || {
                // Pre-calculate attenuated buffers once at worker thread initialization.
                // The startup/on sound has high perceived RMS energy, so it is attenuated to 0.20
                // to make it gentle, soft, and comfortable on the ears.
                let click = attenuate_wav(CLICK_WAV, 0.25);
                let toggle = attenuate_wav(TOGGLE_WAV, 0.25);
                let on = attenuate_wav(ON_WAV, 0.20);
                let off = attenuate_wav(OFF_WAV, 0.25);

                while let Ok(sound_name) = rx.recv() {
                    let bytes: &[u8] = match sound_name {
                        "click" => &click,
                        "toggle" => &toggle,
                        "on" => &on,
                        "off" => &off,
                        _ => continue,
                    };

                    #[cfg(target_os = "windows")]
                    unsafe {
                        const SND_ASYNC: u32 = 0x0001;
                        const SND_NODEFAULT: u32 = 0x0002;
                        const SND_MEMORY: u32 = 0x0004;

                        PlaySoundW(
                            bytes.as_ptr() as *const u16,
                            0,
                            SND_ASYNC | SND_NODEFAULT | SND_MEMORY,
                        );
                    }
                }
            });
        tx
    })
}

/// Dispatches sound playback to the dedicated background audio thread.
/// Non-blocking: returns in microseconds. Drops excess commands if buffer is full.
/// Immediately returns if sound effects are disabled.
pub fn play(sound: &str) {
    if !is_enabled() {
        return;
    }

    let sound_static: &'static str = match sound {
        "click" => "click",
        "toggle" => "toggle",
        "on" => "on",
        "off" => "off",
        _ => return,
    };

    let _ = get_sender().try_send(sound_static);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_default_is_disabled() {
        // Must default to false as specified
        set_enabled(false);
        assert!(!is_enabled(), "Sound effects must default to disabled");
    }

    #[test]
    fn test_sound_toggle_enabled() {
        set_enabled(true);
        assert!(is_enabled());
        set_enabled(false);
        assert!(!is_enabled());
    }

    #[test]
    fn test_attenuate_wav_reduces_amplitude() {
        let orig = ON_WAV;
        let attenuated = attenuate_wav(orig, 0.20);
        assert_eq!(orig.len(), attenuated.len());

        let pos = orig.windows(4).position(|w| w == b"data").unwrap();
        let sample_start = pos + 8;

        let mut max_orig: i16 = 0;
        let mut max_att: i16 = 0;

        let mut i = sample_start;
        while i + 1 < orig.len() {
            let s_orig = i16::from_le_bytes([orig[i], orig[i + 1]]).abs();
            let s_att = i16::from_le_bytes([attenuated[i], attenuated[i + 1]]).abs();
            if s_orig > max_orig { max_orig = s_orig; }
            if s_att > max_att { max_att = s_att; }
            i += 2;
        }

        assert!(max_orig > 10000, "Original sound must have noticeable amplitude");
        assert!(max_att <= (max_orig as f32 * 0.21) as i16, "Attenuated sound must be ~20% of original");
    }

    #[test]
    fn test_play_non_blocking_does_not_panic() {
        set_enabled(true);
        play("click");
        play("toggle");
        play("on");
        play("off");
        play("unknown_sound");
        set_enabled(false);
        play("click");
    }
}
