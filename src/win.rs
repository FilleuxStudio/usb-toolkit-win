//! Petits utilitaires autour de l'API Win32.

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

pub const GENERIC_READ: u32 = 0x8000_0000;
pub const GENERIC_WRITE: u32 = 0x4000_0000;

/// &str -> UTF-16 terminé par un zéro.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Tampon UTF-16 -> String (s'arrête au premier zéro).
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Handle Win32 fermé automatiquement.
pub struct Handle(pub HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// Ouvre un périphérique (`\\.\E:` ou `\\.\PhysicalDrive2`).
pub fn open_device(path: &str, access: u32) -> Result<Handle, u32> {
    let w = wide(path);
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        Err(unsafe { GetLastError() })
    } else {
        Ok(Handle(h))
    }
}

/// Appel DeviceIoControl générique. Retourne le nombre d'octets écrits en sortie.
///
/// # Safety
/// Les pointeurs doivent être valides pour les tailles indiquées.
pub unsafe fn ioctl_raw(
    h: &Handle,
    code: u32,
    input: *const c_void,
    in_len: u32,
    output: *mut c_void,
    out_len: u32,
) -> Result<u32, u32> {
    let mut returned = 0u32;
    let ok = DeviceIoControl(h.0, code, input, in_len, output, out_len, &mut returned, null_mut());
    if ok != 0 {
        Ok(returned)
    } else {
        Err(GetLastError())
    }
}

/// Message lisible pour les codes d'erreur courants.
pub fn explain_error(code: u32) -> String {
    let txt = match code {
        2 => "introuvable",
        5 => "accès refusé (lancez l'outil en administrateur)",
        19 => "le média est protégé en écriture (verrou matériel ou contrôleur)",
        21 => "le périphérique n'est pas prêt (pas de média ?)",
        32 => "le lecteur est utilisé par un autre programme",
        50 => "opération non prise en charge par ce périphérique",
        87 => "paramètre invalide",
        1117 => "erreur d'E/S du périphérique (clé défectueuse ?)",
        _ => "erreur Windows",
    };
    format!("{} (code {})", txt, code)
}
