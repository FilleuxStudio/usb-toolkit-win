//! Opérations : déblocage, renommage, formatage, fichiers cachés, icône.

use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Ioctl::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

use crate::drives;
use crate::win::*;

const ATTR_HIDDEN_SYSTEM: u32 = FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM;

// ───────────────────────── Débloquer la lecture seule ─────────────────────────

pub fn unlock(letter: char) {
    println!("\n[1/4] Politique globale « WriteProtect » (registre)…");
    match clear_registry_write_protect() {
        Ok(true) => println!("      ✔ WriteProtect était à 1 : remis à 0."),
        Ok(false) => println!("      ✔ Aucune protection globale active."),
        Err(e) => println!("      ✘ {}", explain_error(e)),
    }

    println!("[2/4] Attribut READ-ONLY du disque physique…");
    match drives::disk_number(letter) {
        Some(n) => match clear_disk_readonly(n) {
            Ok(()) => println!("      ✔ Attribut disque effacé (disque {}).", n),
            Err(e) => println!("      ✘ {}", explain_error(e)),
        },
        None => println!("      ✘ Numéro de disque introuvable."),
    }

    println!("[3/4] Attribut READ-ONLY du volume (diskpart)…");
    let script = format!(
        "select volume {l}\r\nattributes volume clear readonly\r\nattributes disk clear readonly\r\nexit\r\n",
        l = letter
    );
    match run_diskpart(&script) {
        Ok(out) => {
            for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
                println!("      {}", line);
            }
        }
        Err(e) => println!("      ✘ diskpart a échoué : {}", e),
    }

    println!("[4/4] Test d'écriture réel…");
    match write_test(letter) {
        Ok(()) => println!("      ✔ Écriture possible : la clé n'est plus en lecture seule !"),
        Err(e) => {
            println!("      ✘ Écriture toujours impossible : {}", e);
            println!("\n  Pistes restantes :");
            println!("   • Vérifiez le petit interrupteur de verrouillage physique (clé / adaptateur SD).");
            println!("   • Système de fichiers corrompu : lancez  chkdsk {}: /f", letter);
            println!("   • Beaucoup de clés en fin de vie passent d'office en lecture seule (protection");
            println!("     du contrôleur) : sauvegardez vos données et remplacez la clé.");
        }
    }
}

fn clear_registry_write_protect() -> Result<bool, u32> {
    let sub = wide("SYSTEM\\CurrentControlSet\\Control\\StorageDevicePolicies");
    let name = wide("WriteProtect");
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe {
        RegOpenKeyExW(HKEY_LOCAL_MACHINE, sub.as_ptr(), 0, KEY_SET_VALUE | KEY_QUERY_VALUE, &mut key)
    };
    if rc == 2 {
        return Ok(false); // clé absente : rien à faire
    }
    if rc != 0 {
        return Err(rc);
    }
    let mut value = 0u32;
    let mut size = 4u32;
    let rc = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut value as *mut u32 as *mut u8,
            &mut size,
        )
    };
    let mut changed = false;
    if rc == 0 && value != 0 {
        let zero = 0u32;
        let rc = unsafe {
            RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, &zero as *const u32 as *const u8, 4)
        };
        if rc != 0 {
            unsafe { RegCloseKey(key) };
            return Err(rc);
        }
        changed = true;
    }
    unsafe { RegCloseKey(key) };
    Ok(changed)
}

fn clear_disk_readonly(disk: u32) -> Result<(), u32> {
    let h = open_device(&format!("\\\\.\\PhysicalDrive{}", disk), GENERIC_READ | GENERIC_WRITE)?;
    let mut s: SET_DISK_ATTRIBUTES = unsafe { zeroed() };
    s.Version = size_of::<SET_DISK_ATTRIBUTES>() as u32;
    s.Persist = 1;
    s.Attributes = 0;
    s.AttributesMask = 1; // DISK_ATTRIBUTE_READ_ONLY
    unsafe {
        ioctl_raw(
            &h,
            IOCTL_DISK_SET_DISK_ATTRIBUTES,
            &s as *const _ as *const c_void,
            size_of::<SET_DISK_ATTRIBUTES>() as u32,
            std::ptr::null_mut(),
            0,
        )?;
    }
    Ok(())
}

fn run_diskpart(script: &str) -> Result<String, String> {
    let path = std::env::temp_dir().join("usb_toolkit_diskpart.txt");
    fs::write(&path, script).map_err(|e| e.to_string())?;
    let out = Command::new("diskpart").arg("/s").arg(&path).output().map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&path);
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Tente de créer puis supprimer un fichier à la racine.
pub fn write_test(letter: char) -> std::io::Result<()> {
    let p = PathBuf::from(format!("{}:\\", letter)).join("usbtoolkit_write_test.tmp");
    let mut f = fs::File::create(&p)?;
    f.write_all(b"test")?;
    f.sync_all()?;
    drop(f);
    fs::remove_file(&p)
}

// ───────────────────────── Renommer ─────────────────────────

/// Vérifie qu'un nom de volume est valide pour le système de fichiers visé.
pub fn validate_label(label: &str, fs_name: &str) -> Result<(), String> {
    let max = if fs_name.eq_ignore_ascii_case("NTFS") { 32 } else { 11 };
    if label.chars().count() > max {
        return Err(format!("{} : le nom est limité à {} caractères.", fs_name, max));
    }
    if label.chars().any(|c| c.is_control() || "\\/:*?\"<>|%^&".contains(c)) {
        return Err("Caractères interdits : \\ / : * ? \" < > | % ^ &".into());
    }
    Ok(())
}

pub fn rename(letter: char, new_label: &str, fs_name: &str) -> Result<(), String> {
    validate_label(new_label, fs_name)?;
    let root = wide(&format!("{}:\\", letter));
    let label = wide(new_label);
    if unsafe { SetVolumeLabelW(root.as_ptr(), label.as_ptr()) } == 0 {
        return Err(explain_error(unsafe { windows_sys::Win32::Foundation::GetLastError() }));
    }
    Ok(())
}

// ───────────────────────── Formater / convertir ─────────────────────────

pub fn format_drive(letter: char, fs_name: &str, label: &str, quick: bool) -> Result<(), String> {
    validate_label(label, fs_name)?;
    let mut cmd = Command::new("format");
    cmd.arg(format!("{}:", letter))
        .arg(format!("/FS:{}", fs_name))
        .arg(format!("/V:{}", label))
        .arg("/Y");
    if quick {
        cmd.arg("/Q");
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "format.com a échoué ({}). Fermez l'Explorateur / les programmes qui utilisent la clé. \
             Rappel : Windows refuse FAT32 au-delà de 32 Go (utilisez exFAT ou NTFS).",
            status
        ))
    }
}

pub fn convert_to_ntfs(letter: char) -> Result<(), String> {
    let status = Command::new("convert")
        .arg(format!("{}:", letter))
        .arg("/FS:NTFS")
        .arg("/X")
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("convert a échoué ({})", status)) }
}

// ───────────────────────── Fichiers cachés ─────────────────────────

fn attrs(p: &Path) -> Option<u32> {
    let w = wide(&p.to_string_lossy());
    let a = unsafe { GetFileAttributesW(w.as_ptr()) };
    if a == INVALID_FILE_ATTRIBUTES { None } else { Some(a) }
}

fn set_attrs(p: &Path, a: u32) -> Result<(), String> {
    let w = wide(&p.to_string_lossy());
    if unsafe { SetFileAttributesW(w.as_ptr(), a) } == 0 {
        Err(explain_error(unsafe { windows_sys::Win32::Foundation::GetLastError() }))
    } else {
        Ok(())
    }
}

/// Construit un chemin sous la racine du lecteur en refusant `..`, `:` et chemins absolus.
pub fn safe_path(letter: char, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_start_matches(['\\', '/']);
    if rel.is_empty() || rel.contains("..") || rel.contains(':') {
        return Err("Chemin invalide (relatif à la racine de la clé uniquement).".into());
    }
    Ok(PathBuf::from(format!("{}:\\", letter)).join(rel))
}

pub fn create_hidden_file(letter: char, rel: &str, content: &str, system: bool) -> Result<PathBuf, String> {
    let p = safe_path(letter, rel)?;
    if let Some(a) = attrs(&p) {
        set_attrs(&p, a & !ATTR_HIDDEN_SYSTEM)?; // permet d'écraser un fichier déjà caché
    }
    fs::write(&p, content).map_err(|e| e.to_string())?;
    hide(&p, system)?;
    Ok(p)
}

pub fn create_hidden_dir(letter: char, rel: &str, system: bool) -> Result<PathBuf, String> {
    let p = safe_path(letter, rel)?;
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    hide(&p, system)?;
    Ok(p)
}

pub fn hide(p: &Path, system: bool) -> Result<(), String> {
    let a = attrs(p).ok_or("Élément introuvable.")?;
    let extra = if system { ATTR_HIDDEN_SYSTEM } else { FILE_ATTRIBUTE_HIDDEN };
    set_attrs(p, a | extra)
}

pub fn unhide(p: &Path) -> Result<(), String> {
    let a = attrs(p).ok_or("Élément introuvable.")?;
    let mut n = a & !ATTR_HIDDEN_SYSTEM;
    if n == 0 {
        n = FILE_ATTRIBUTE_NORMAL;
    }
    set_attrs(p, n)
}

/// Liste récursive de tout ce qui est caché/système (hors dossiers système de Windows).
pub fn scan_hidden(letter: char) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(&PathBuf::from(format!("{}:\\", letter)), &mut out);
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name == "system volume information" || name == "$recycle.bin" {
            continue;
        }
        if let Some(a) = attrs(&p) {
            if a & ATTR_HIDDEN_SYSTEM != 0 {
                out.push(p.clone());
            }
            if a & FILE_ATTRIBUTE_DIRECTORY != 0 && a & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                walk(&p, out);
            }
        }
    }
}

// ───────────────────────── Icône du lecteur ─────────────────────────

const ICON_NAME: &str = "usb_icon.ico";

/// Icône de l'application, embarquée dans l'exécutable.
pub const BUILTIN_ICON: &[u8] = include_bytes!("../assets/icon.ico");

pub fn set_drive_icon_from_file(letter: char, ico: &Path, display_label: Option<&str>) -> Result<(), String> {
    let data = fs::read(ico).map_err(|e| format!("Lecture de l'icône impossible : {}", e))?;
    set_drive_icon(letter, &data, display_label)
}

pub fn set_drive_icon(letter: char, data: &[u8], display_label: Option<&str>) -> Result<(), String> {
    if data.len() < 6 || data[0..4] != [0, 0, 1, 0] {
        return Err("Ce fichier n'est pas un .ico valide.".into());
    }
    let root = PathBuf::from(format!("{}:\\", letter));
    let icon_dst = root.join(ICON_NAME);
    let inf = root.join("autorun.inf");

    for p in [&icon_dst, &inf] {
        if let Some(a) = attrs(p) {
            set_attrs(p, a & !(ATTR_HIDDEN_SYSTEM | FILE_ATTRIBUTE_READONLY))?;
        }
    }
    fs::write(&icon_dst, data).map_err(|e| e.to_string())?;

    let mut content = format!("[autorun]\r\nicon={}\r\n", ICON_NAME);
    if let Some(l) = display_label {
        if l.is_ascii() && !l.is_empty() {
            content.push_str(&format!("label={}\r\n", l));
        }
    }
    fs::write(&inf, content).map_err(|e| e.to_string())?;
    set_attrs(&icon_dst, ATTR_HIDDEN_SYSTEM | FILE_ATTRIBUTE_READONLY)?;
    set_attrs(&inf, ATTR_HIDDEN_SYSTEM | FILE_ATTRIBUTE_READONLY)?;
    refresh_shell();
    Ok(())
}

pub fn remove_drive_icon(letter: char) -> Result<(), String> {
    let root = PathBuf::from(format!("{}:\\", letter));
    for name in [ICON_NAME, "autorun.inf"] {
        let p = root.join(name);
        if attrs(&p).is_some() {
            set_attrs(&p, FILE_ATTRIBUTE_NORMAL)?;
            fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    refresh_shell();
    Ok(())
}

/// Demande à l'Explorateur de recharger les icônes.
fn refresh_shell() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED as _, SHCNF_IDLIST as _, std::ptr::null(), std::ptr::null());
    }
}

// ───────────────────────── Infos avancées (PowerShell/WMI) ─────────────────────────

pub fn advanced_info(disk: u32) {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$n = __N__
Write-Host '── Win32_DiskDrive (WMI) ──'
$d = Get-CimInstance Win32_DiskDrive | Where-Object { $_.Index -eq $n }
$d | Format-List Model, Manufacturer, InterfaceType, MediaType, FirmwareRevision, SerialNumber, Size, BytesPerSector, TotalSectors, Partitions, Status, PNPDeviceID
Write-Host '── Get-Disk ──'
Get-Disk -Number $n | Format-List FriendlyName, PartitionStyle, HealthStatus, OperationalStatus, IsReadOnly, IsOffline, BusType, LogicalSectorSize, PhysicalSectorSize, Signature, Guid
Write-Host '── Partitions ──'
Get-Partition -DiskNumber $n | Format-Table PartitionNumber, DriveLetter, Type, Size, Offset, IsActive, IsHidden -AutoSize
Write-Host '── Périphérique USB parent (VID/PID) ──'
$parent = (Get-PnpDeviceProperty -InstanceId $d.PNPDeviceID -KeyName 'DEVPKEY_Device_Parent').Data
Write-Host "Instance parent : $parent"
if ($parent -match 'VID_([0-9A-F]{4})&PID_([0-9A-F]{4})') { Write-Host ("VID = 0x{0}   PID = 0x{1}" -f $Matches[1], $Matches[2]) }
foreach ($k in 'DEVPKEY_Device_Manufacturer','DEVPKEY_Device_BusReportedDeviceDesc','DEVPKEY_Device_FirstInstallDate','DEVPKEY_Device_LastArrivalDate','DEVPKEY_Device_DriverVersion') {
  $v = (Get-PnpDeviceProperty -InstanceId $parent -KeyName $k).Data
  if ($v) { Write-Host ("{0,-40} {1}" -f $k.Replace('DEVPKEY_Device_',''), $v) }
}
"#
    .replace("__N__", &disk.to_string());

    let path = std::env::temp_dir().join("usb_toolkit_info.ps1");
    if fs::write(&path, script).is_err() {
        println!("Impossible d'écrire le script temporaire.");
        return;
    }
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&path)
        .status();
    let _ = fs::remove_file(&path);
}
