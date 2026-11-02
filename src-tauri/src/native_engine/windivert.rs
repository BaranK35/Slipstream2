//! Direct WinDivert FFI bindings and dynamic library loader in safe/robust Rust.
//! Targets WinDivert 2.2+ x64 driver and user-mode library.

use std::{
    ffi::{c_char, c_void, CString},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

// WinDivert Layer Constants
pub const WINDIVERT_LAYER_NETWORK: u32 = 0;
pub const WINDIVERT_LAYER_NETWORK_FORWARD: u32 = 1;
pub const WINDIVERT_LAYER_FLOW: u32 = 2;
pub const WINDIVERT_LAYER_SOCKET: u32 = 3;
pub const WINDIVERT_LAYER_REFLECT: u32 = 4;

// WinDivert Open Flags
pub const WINDIVERT_FLAG_SNIFF: u64 = 0x0001;
pub const WINDIVERT_FLAG_DROP: u64 = 0x0002;
pub const WINDIVERT_FLAG_RECV_ONLY: u64 = 0x0004;
pub const WINDIVERT_FLAG_READ_ONLY: u64 = WINDIVERT_FLAG_RECV_ONLY;
pub const WINDIVERT_FLAG_SEND_ONLY: u64 = 0x0008;
pub const WINDIVERT_FLAG_WRITE_ONLY: u64 = WINDIVERT_FLAG_SEND_ONLY;
pub const WINDIVERT_FLAG_NO_INSTALL: u64 = 0x0010;
pub const WINDIVERT_FLAG_FRAGMENTS: u64 = 0x0020;

// WinDivert Shutdown Flags
pub const WINDIVERT_SHUTDOWN_RECV: u32 = 1;
pub const WINDIVERT_SHUTDOWN_SEND: u32 = 2;
pub const WINDIVERT_SHUTDOWN_BOTH: u32 = 3;

// WinDivert Helper Checksum Flags
pub const WINDIVERT_HELPER_NO_IP_CHECKSUM: u64 = 1;
pub const WINDIVERT_HELPER_NO_ICMP_CHECKSUM: u64 = 2;
pub const WINDIVERT_HELPER_NO_ICMPV6_CHECKSUM: u64 = 4;
pub const WINDIVERT_HELPER_NO_TCP_CHECKSUM: u64 = 8;
pub const WINDIVERT_HELPER_NO_UDP_CHECKSUM: u64 = 16;

/// WinDivert 2.2 address structure matching `WINDIVERT_ADDRESS` in C.
/// Size: exactly 80 bytes. Alignment: 8 bytes.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug)]
pub struct WinDivertAddress {
    pub timestamp: i64,
    pub flags: u64,
    pub data: [u8; 64],
}

impl Default for WinDivertAddress {
    fn default() -> Self {
        Self {
            timestamp: 0,
            flags: 0,
            data: [0u8; 64],
        }
    }
}

impl WinDivertAddress {
    #[inline]
    pub fn layer(&self) -> u8 {
        (self.flags & 0xFF) as u8
    }

    #[inline]
    pub fn set_layer(&mut self, layer: u8) {
        self.flags = (self.flags & !0xFF) | (layer as u64);
    }

    #[inline]
    pub fn event(&self) -> u8 {
        ((self.flags >> 8) & 0xFF) as u8
    }

    #[inline]
    pub fn set_event(&mut self, event: u8) {
        self.flags = (self.flags & !(0xFF << 8)) | ((event as u64) << 8);
    }

    #[inline]
    pub fn is_sniffed(&self) -> bool {
        (self.flags & (1 << 16)) != 0
    }

    #[inline]
    pub fn set_sniffed(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 16;
        } else {
            self.flags &= !(1 << 16);
        }
    }

    #[inline]
    pub fn is_outbound(&self) -> bool {
        (self.flags & (1 << 17)) != 0
    }

    #[inline]
    pub fn set_outbound(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 17;
        } else {
            self.flags &= !(1 << 17);
        }
    }

    #[inline]
    pub fn is_loopback(&self) -> bool {
        (self.flags & (1 << 18)) != 0
    }

    #[inline]
    pub fn set_loopback(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 18;
        } else {
            self.flags &= !(1 << 18);
        }
    }

    #[inline]
    pub fn is_impostor(&self) -> bool {
        (self.flags & (1 << 19)) != 0
    }

    #[inline]
    pub fn set_impostor(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 19;
        } else {
            self.flags &= !(1 << 19);
        }
    }

    #[inline]
    pub fn is_ipv6(&self) -> bool {
        (self.flags & (1 << 20)) != 0
    }

    #[inline]
    pub fn set_ipv6(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 20;
        } else {
            self.flags &= !(1 << 20);
        }
    }

    #[inline]
    pub fn ip_checksum(&self) -> bool {
        (self.flags & (1 << 21)) != 0
    }

    #[inline]
    pub fn set_ip_checksum(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 21;
        } else {
            self.flags &= !(1 << 21);
        }
    }

    #[inline]
    pub fn tcp_checksum(&self) -> bool {
        (self.flags & (1 << 22)) != 0
    }

    #[inline]
    pub fn set_tcp_checksum(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 22;
        } else {
            self.flags &= !(1 << 22);
        }
    }

    #[inline]
    pub fn udp_checksum(&self) -> bool {
        (self.flags & (1 << 23)) != 0
    }

    #[inline]
    pub fn set_udp_checksum(&mut self, val: bool) {
        if val {
            self.flags |= 1 << 23;
        } else {
            self.flags &= !(1 << 23);
        }
    }

    #[inline]
    pub fn if_idx(&self) -> u32 {
        u32::from_ne_bytes(self.data[0..4].try_into().unwrap_or([0; 4]))
    }

    #[inline]
    pub fn set_if_idx(&mut self, idx: u32) {
        self.data[0..4].copy_from_slice(&idx.to_ne_bytes());
    }

    #[inline]
    pub fn sub_if_idx(&self) -> u32 {
        u32::from_ne_bytes(self.data[4..8].try_into().unwrap_or([0; 4]))
    }

    #[inline]
    pub fn set_sub_if_idx(&mut self, idx: u32) {
        self.data[4..8].copy_from_slice(&idx.to_ne_bytes());
    }
}

// Function pointer signatures for WinDivert C API
pub type FnWinDivertOpen = unsafe extern "system" fn(
    filter: *const c_char,
    layer: u32,
    priority: i16,
    flags: u64,
) -> isize;

pub type FnWinDivertRecv = unsafe extern "system" fn(
    handle: isize,
    packet: *mut u8,
    packet_len: u32,
    recv_len: *mut u32,
    addr: *mut WinDivertAddress,
) -> i32;

pub type FnWinDivertSend = unsafe extern "system" fn(
    handle: isize,
    packet: *const u8,
    packet_len: u32,
    send_len: *mut u32,
    addr: *const WinDivertAddress,
) -> i32;

pub type FnWinDivertHelperCalcChecksums = unsafe extern "system" fn(
    packet: *mut u8,
    packet_len: u32,
    addr: *mut WinDivertAddress,
    flags: u64,
) -> i32;

pub type FnWinDivertClose = unsafe extern "system" fn(handle: isize) -> i32;

pub type FnWinDivertShutdown = unsafe extern "system" fn(handle: isize, how: u32) -> i32;

pub const WINDIVERT_PARAM_QUEUE_LENGTH: u32 = 0;
pub const WINDIVERT_PARAM_QUEUE_TIME: u32 = 1;
pub const WINDIVERT_PARAM_QUEUE_SIZE: u32 = 2;

pub type FnWinDivertSetParam = unsafe extern "system" fn(handle: isize, param: u32, value: u64) -> i32;
pub type FnWinDivertGetParam = unsafe extern "system" fn(handle: isize, param: u32, value: *mut u64) -> i32;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(lpLibFileName: *const u16) -> isize;
    fn GetProcAddress(hModule: isize, lpProcName: *const c_char) -> *const c_void;
    fn FreeLibrary(hLibModule: isize) -> i32;
    fn SetDllDirectoryW(lpPathName: *const u16) -> i32;
    fn GetLastError() -> u32;
}

/// Dynamic wrapper holding function pointers to `WinDivert.dll`.
pub struct WinDivertApi {
    pub open_fn: FnWinDivertOpen,
    pub recv_fn: FnWinDivertRecv,
    pub send_fn: FnWinDivertSend,
    pub calc_checksums_fn: FnWinDivertHelperCalcChecksums,
    pub close_fn: FnWinDivertClose,
    pub shutdown_fn: Option<FnWinDivertShutdown>,
    pub set_param_fn: Option<FnWinDivertSetParam>,
    pub get_param_fn: Option<FnWinDivertGetParam>,
    h_module: isize,
}

impl Drop for WinDivertApi {
    fn drop(&mut self) {
        if self.h_module != 0 && self.h_module != -1 {
            unsafe {
                FreeLibrary(self.h_module);
            }
        }
    }
}

unsafe impl Send for WinDivertApi {}
unsafe impl Sync for WinDivertApi {}

impl WinDivertApi {
    /// Loads `WinDivert.dll` dynamically from the specified path or standard search path.
    pub fn load(custom_path: Option<&Path>) -> Result<Arc<Self>, String> {
        let dll_path = if let Some(p) = custom_path {
            p.to_path_buf()
        } else {
            PathBuf::from("WinDivert.dll")
        };

        if let Some(parent) = dll_path.parent() {
            if parent.exists() {
                let wide_parent: Vec<u16> = parent
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                unsafe {
                    SetDllDirectoryW(wide_parent.as_ptr());
                }
            }
        }

        use std::os::windows::ffi::OsStrExt;
        let wide_path: Vec<u16> = dll_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let h_module = unsafe { LoadLibraryW(wide_path.as_ptr()) };
        if h_module == 0 || h_module == -1 {
            let code = unsafe { GetLastError() };
            return Err(format!(
                "WinDivert.dll yüklenemedi (Hata kodu: {code}, Yol: {}). Dosya eksik veya antivirüs tarafından engellenmiş olabilir.",
                dll_path.display()
            ));
        }

        let load_symbol = |name: &str| -> Result<*const c_void, String> {
            let c_name = CString::new(name).map_err(|e| e.to_string())?;
            let addr = unsafe { GetProcAddress(h_module, c_name.as_ptr()) };
            if addr.is_null() {
                Err(format!("WinDivert sembolü bulunamadı: {name}"))
            } else {
                Ok(addr)
            }
        };

        let open_fn: FnWinDivertOpen = unsafe { std::mem::transmute(load_symbol("WinDivertOpen")?) };
        let recv_fn: FnWinDivertRecv = unsafe { std::mem::transmute(load_symbol("WinDivertRecv")?) };
        let send_fn: FnWinDivertSend = unsafe { std::mem::transmute(load_symbol("WinDivertSend")?) };
        let calc_checksums_fn: FnWinDivertHelperCalcChecksums =
            unsafe { std::mem::transmute(load_symbol("WinDivertHelperCalcChecksums")?) };
        let close_fn: FnWinDivertClose = unsafe { std::mem::transmute(load_symbol("WinDivertClose")?) };

        let shutdown_fn: Option<FnWinDivertShutdown> = if let Ok(addr) = load_symbol("WinDivertShutdown") {
            Some(unsafe { std::mem::transmute(addr) })
        } else {
            None
        };

        let set_param_fn: Option<FnWinDivertSetParam> = if let Ok(addr) = load_symbol("WinDivertSetParam") {
            Some(unsafe { std::mem::transmute(addr) })
        } else {
            None
        };

        let get_param_fn: Option<FnWinDivertGetParam> = if let Ok(addr) = load_symbol("WinDivertGetParam") {
            Some(unsafe { std::mem::transmute(addr) })
        } else {
            None
        };

        Ok(Arc::new(Self {
            open_fn,
            recv_fn,
            send_fn,
            calc_checksums_fn,
            close_fn,
            shutdown_fn,
            set_param_fn,
            get_param_fn,
            h_module,
        }))
    }

    /// Opens a WinDivert handle.
    pub fn open(
        self: &Arc<Self>,
        filter: &str,
        layer: u32,
        priority: i16,
        flags: u64,
    ) -> Result<WinDivertHandle, String> {
        let c_filter = CString::new(filter).map_err(|e| format!("Geçersiz filtre dizesi: {e}"))?;
        let handle = unsafe { (self.open_fn)(c_filter.as_ptr(), layer, priority, flags) };
        if handle == 0 || handle == -1 {
            let code = unsafe { GetLastError() };
            let msg = match code {
                5 => "Yönetici yetkisi gerekli (Erişim Engellendi).",
                2 => "WinDivert64.sys sürücü dosyası bulunamadı.",
                577 => "Windows dijital sürücü imza zorunluluğu WinDivert sürücüsünü engelledi.",
                10 => "Ağ filtresi veya WFP başka bir uygulama tarafından kilitli.",
                _ => "WinDivert filtresi açılamadı.",
            };
            return Err(format!("{msg} (Win32 Hata Kodu: {code})"));
        }

        Ok(WinDivertHandle {
            handle,
            api: Arc::clone(self),
            is_closed: AtomicBool::new(false),
        })
    }
}

/// An opened RAII handle to the WinDivert driver.
pub struct WinDivertHandle {
    handle: isize,
    api: Arc<WinDivertApi>,
    is_closed: AtomicBool,
}

unsafe impl Send for WinDivertHandle {}
unsafe impl Sync for WinDivertHandle {}

impl WinDivertHandle {
    /// Receives a packet intercepted by the driver.
    pub fn recv(
        &self,
        packet_buf: &mut [u8],
        addr: &mut WinDivertAddress,
    ) -> Result<usize, u32> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(995); // ERROR_OPERATION_ABORTED
        }

        let mut recv_len: u32 = 0;
        let ok = unsafe {
            (self.api.recv_fn)(
                self.handle,
                packet_buf.as_mut_ptr(),
                packet_buf.len() as u32,
                &mut recv_len,
                addr as *mut WinDivertAddress,
            )
        };

        if ok != 0 {
            Ok(recv_len as usize)
        } else {
            let code = unsafe { GetLastError() };
            Err(code)
        }
    }

    /// Sends or reinjects a packet into the network stack.
    pub fn send(
        &self,
        packet: &[u8],
        addr: &WinDivertAddress,
    ) -> Result<usize, u32> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(995);
        }

        let mut send_len: u32 = 0;
        let ok = unsafe {
            (self.api.send_fn)(
                self.handle,
                packet.as_ptr(),
                packet.len() as u32,
                &mut send_len,
                addr as *const WinDivertAddress,
            )
        };

        if ok != 0 {
            Ok(send_len as usize)
        } else {
            let code = unsafe { GetLastError() };
            Err(code)
        }
    }

    /// Helper to recalculate IP, TCP, and UDP checksums.
    pub fn calc_checksums(
        &self,
        packet: &mut [u8],
        addr: &mut WinDivertAddress,
        flags: u64,
    ) -> bool {
        let ok = unsafe {
            (self.api.calc_checksums_fn)(
                packet.as_mut_ptr(),
                packet.len() as u32,
                addr as *mut WinDivertAddress,
                flags,
            )
        };
        ok != 0
    }

    /// Sets WinDivert driver parameter (e.g. queue length, queue time, queue size).
    pub fn set_param(&self, param: u32, value: u64) -> bool {
        if self.is_closed.load(Ordering::Relaxed) {
            return false;
        }
        if let Some(set_fn) = self.api.set_param_fn {
            unsafe { set_fn(self.handle, param, value) != 0 }
        } else {
            false
        }
    }

    /// Queries current WinDivert driver parameter.
    pub fn get_param(&self, param: u32) -> Option<u64> {
        if self.is_closed.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(get_fn) = self.api.get_param_fn {
            let mut val = 0u64;
            let ok = unsafe { get_fn(self.handle, param, &mut val) };
            if ok != 0 {
                Some(val)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Unblocks pending `recv` operations immediately.
    pub fn shutdown(&self, how: u32) {
        if let Some(shutdown_fn) = self.api.shutdown_fn {
            unsafe {
                shutdown_fn(self.handle, how);
            }
        }
    }

    /// Closes the WinDivert handle.
    pub fn close(&self) {
        if !self.is_closed.swap(true, Ordering::SeqCst) {
            self.shutdown(WINDIVERT_SHUTDOWN_BOTH);
            unsafe {
                (self.api.close_fn)(self.handle);
            }
        }
    }

    pub fn raw_handle(&self) -> isize {
        self.handle
    }
}

impl Drop for WinDivertHandle {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windivert_address_size_and_alignment() {
        assert_eq!(
            std::mem::size_of::<WinDivertAddress>(),
            80,
            "WINDIVERT_ADDRESS must be exactly 80 bytes in WinDivert 2.2"
        );
        assert_eq!(
            std::mem::align_of::<WinDivertAddress>(),
            8,
            "WINDIVERT_ADDRESS must be 8-byte aligned"
        );
    }

    #[test]
    fn test_windivert_address_flags_and_properties() {
        let mut addr = WinDivertAddress::default();
        assert_eq!(addr.layer(), 0);
        assert_eq!(addr.event(), 0);
        assert!(!addr.is_outbound());
        assert!(!addr.is_loopback());
        assert!(!addr.is_impostor());
        assert!(!addr.is_ipv6());

        addr.set_layer(WINDIVERT_LAYER_NETWORK as u8);
        addr.set_event(3);
        addr.set_outbound(true);
        addr.set_loopback(false);
        addr.set_impostor(true);
        addr.set_ipv6(true);
        addr.set_tcp_checksum(true);

        assert_eq!(addr.layer(), WINDIVERT_LAYER_NETWORK as u8);
        assert_eq!(addr.event(), 3);
        assert!(addr.is_outbound());
        assert!(!addr.is_loopback());
        assert!(addr.is_impostor());
        assert!(addr.is_ipv6());
        assert!(addr.tcp_checksum());

        addr.set_outbound(false);
        assert!(!addr.is_outbound());
    }

    #[test]
    fn test_windivert_network_interface_indexes() {
        let mut addr = WinDivertAddress::default();
        addr.set_if_idx(42);
        addr.set_sub_if_idx(7);

        assert_eq!(addr.if_idx(), 42);
        assert_eq!(addr.sub_if_idx(), 7);
    }
}
