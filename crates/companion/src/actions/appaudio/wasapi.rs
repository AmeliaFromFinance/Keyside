//! Windows through Core Audio. Each app that plays sound has a session on each output device,
//! the same ones the Volume Mixer shows. Windows has no simple change feed, so this looks every
//! second and a half.

use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::{
    AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
use windows::core::{Interface, PWSTR};

use super::Stream;

pub const UNAVAILABLE: &str = "Not available";

struct Session {
    pid: u32,
    volume: ISimpleAudioVolume,
}

/// Every app session on every active output.
fn sessions() -> windows::core::Result<Vec<Session>> {
    unsafe {
        // Already set up on this thread is fine too.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let outputs = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut out = Vec::new();
        for i in 0..outputs.GetCount()? {
            let Ok(manager) = outputs.Item(i).and_then(|d| d.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)) else { continue };
            let Ok(list) = manager.GetSessionEnumerator() else { continue };
            for j in 0..list.GetCount().unwrap_or(0) {
                let Ok(control) = list.GetSession(j) else { continue };
                if control.GetState().is_ok_and(|s| s == AudioSessionStateExpired) {
                    continue;
                }
                let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
                if control2.IsSystemSoundsSession() == S_OK {
                    continue;
                }
                let (Ok(pid), Ok(volume)) = (control2.GetProcessId(), control.cast::<ISimpleAudioVolume>()) else { continue };
                if pid != 0 {
                    out.push(Session { pid, volume });
                }
            }
        }
        Ok(out)
    }
}

/// The program's file name, like "Spotify.exe".
fn exe(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let found = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        found.ok()?;
        let path = String::from_utf16_lossy(buf.get(..len as usize)?);
        path.rsplit('\\').next().filter(|n| !n.is_empty()).map(str::to_string)
    }
}

/// Core Audio calls block, so they run off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> windows::core::Result<T> + Send + 'static) -> Result<T, String> {
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r.map_err(|e| e.message()),
        Err(_) => Err("audio check failed".into()),
    }
}

pub async fn available() -> bool {
    blocking(|| sessions().map(|_| ())).await.is_ok()
}

pub async fn list() -> Vec<Stream> {
    blocking(|| {
        let mut out: Vec<Stream> = Vec::new();
        for s in sessions()? {
            let muted = unsafe { s.volume.GetMute()?.as_bool() };
            let volume = f64::from(unsafe { s.volume.GetMasterVolume()? });
            // One entry per app, even when it plays on several outputs.
            if let Some(seen) = out.iter_mut().find(|x| x.index == s.pid) {
                seen.muted &= muted;
                seen.volume = seen.volume.max(volume);
                continue;
            }
            let Some(binary) = exe(s.pid) else { continue };
            let name = binary.strip_suffix(".exe").or_else(|| binary.strip_suffix(".EXE")).unwrap_or(&binary).to_string();
            out.push(Stream { index: s.pid, name, binary, muted, volume });
        }
        out.sort_by_key(|s| s.index);
        Ok(out)
    })
    .await
    .unwrap_or_default()
}

pub async fn set_mute(pids: &[u32], on: bool) -> Result<(), String> {
    let pids = pids.to_vec();
    blocking(move || {
        for s in sessions()?.into_iter().filter(|s| pids.contains(&s.pid)) {
            unsafe { s.volume.SetMute(on, std::ptr::null())? };
        }
        Ok(())
    })
    .await
}

pub async fn set_volume(pids: &[u32], value: f64) -> Result<(), String> {
    let pids = pids.to_vec();
    blocking(move || {
        for s in sessions()?.into_iter().filter(|s| pids.contains(&s.pid)) {
            unsafe { s.volume.SetMasterVolume(value as f32, std::ptr::null())? };
        }
        Ok(())
    })
    .await
}

pub struct Watcher {
    tick: tokio::time::Interval,
}

impl Watcher {
    pub fn new() -> Self {
        let mut tick = tokio::time::interval(Duration::from_millis(1500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Self { tick }
    }

    pub async fn changed(&mut self) {
        self.tick.tick().await;
    }
}
