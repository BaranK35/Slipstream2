//! Ultra-low latency native Windows sound effects.

pub const CLICK_WAV: &[u8] = include_bytes!("../sounds/click.wav");
pub const TOGGLE_WAV: &[u8] = include_bytes!("../sounds/toggle.wav");
pub const ON_WAV: &[u8] = include_bytes!("../sounds/on.wav");
pub const OFF_WAV: &[u8] = include_bytes!("../sounds/off.wav");

#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn PlaySoundW(pszSound: *const u16, hmod: isize, fdwSound: u32) -> i32;
}

pub fn play(sound: &str) {
    #[cfg(target_os = "windows")]
    {
        const SND_ASYNC: u32 = 0x0001;
        const SND_NODEFAULT: u32 = 0x0002;
        const SND_MEMORY: u32 = 0x0004;

        let bytes: &[u8] = match sound {
            "click" => CLICK_WAV,
            "toggle" => TOGGLE_WAV,
            "on" => ON_WAV,
            "off" => OFF_WAV,
            _ => return,
        };

        unsafe {
            PlaySoundW(bytes.as_ptr() as *const u16, 0, SND_ASYNC | SND_NODEFAULT | SND_MEMORY);
        }
    }
}
