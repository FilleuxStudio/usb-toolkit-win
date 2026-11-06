//! Détection des clés USB et collecte d'informations détaillées.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null_mut, read_unaligned};

use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Ioctl::*;

use crate::win::*;

// Types de lecteur retournés par GetDriveTypeW.
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

// Flags de GetVolumeInformationW (valeurs documentées par Microsoft).
const FILE_CASE_SENSITIVE_SEARCH: u32 = 0x0000_0001;
const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
const FILE_FILE_COMPRESSION: u32 = 0x0000_0010;
const FILE_SUPPORTS_SPARSE_FILES: u32 = 0x0000_0040;
const FILE_SUPPORTS_REPARSE_POINTS: u32 = 0x0000_0080;
const FILE_SUPPORTS_ENCRYPTION: u32 = 0x0002_0000;
const FILE_NAMED_STREAMS: u32 = 0x0004_0000;
const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;
const FILE_SUPPORTS_HARD_LINKS: u32 = 0x0040_0000;

#[derive(Default, Clone)]
pub struct DriveInfo {
    pub letter: char,
    pub drive_type: u32,
    pub has_media: bool,
    // Volume / système de fichiers
    pub label: String,
    pub fs: String,
    pub serial: u32,
    pub fs_flags: u32,
    pub max_component: u32,
    pub volume_guid: String,
    pub total: u64,
    pub free: u64,
    pub cluster_bytes: u32,
    pub sector_bytes: u32,
    // Périphérique physique
    pub vendor: String,
    pub product: String,
    pub revision: String,
    pub device_serial: String,
    pub bus_type: i32,
    pub removable_media: bool,
    pub disk_number: Option<u32>,
    pub disk_size: Option<u64>,
    pub disk_readonly: Option<bool>,
    // Partition
    pub part_style: Option<i32>,
    pub part_offset: Option<i64>,
    pub part_length: Option<i64>,
    pub part_number: Option<u32>,
}

impl DriveInfo {
    pub fn root(&self) -> String {
        format!("{}:\\", self.letter)
    }
    pub fn volume_readonly(&self) -> bool {
        self.fs_flags & FILE_READ_ONLY_VOLUME != 0
    }
}

pub fn bus_name(b: i32) -> &'static str {
    match b {
        0 => "Inconnu",
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        4 => "IEEE 1394",
        5 => "SSA",
        6 => "Fibre Channel",
        7 => "USB",
        8 => "RAID",
        9 => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        12 => "Carte SD",
        13 => "MMC",
        14 => "Virtuel",
        15 => "Virtuel (fichier)",
        16 => "Storage Spaces",
        17 => "NVMe",
        _ => "Autre",
    }
}

pub fn drive_type_name(t: u32) -> &'static str {
    match t {
        DRIVE_REMOVABLE => "Amovible",
        DRIVE_FIXED => "Fixe",
        DRIVE_REMOTE => "Réseau",
        DRIVE_CDROM => "CD/DVD",
        DRIVE_RAMDISK => "RAM disk",
        _ => "Inconnu",
    }
}

pub fn part_style_name(s: i32) -> &'static str {
    match s {
        0 => "MBR",
        1 => "GPT",
        2 => "RAW (sans table de partitions)",
        _ => "Inconnu",
    }
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Kio", "Mio", "Gio", "Tio"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.2} {}", v, UNITS[i])
}

fn cstr_at(buf: &[u8], off: u32) -> String {
    let off = off as usize;
    if off == 0 || off >= buf.len() {
        return String::new();
    }
    let s = &buf[off..];
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).trim().to_string()
}

/// Lettres des lecteurs USB / amovibles actuellement branchés.
pub fn list_usb_drives() -> Vec<char> {
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 2..26u32 {
        // on saute A: et B: (disquettes)
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = wide(&format!("{}:\\", letter));
        let t = unsafe { GetDriveTypeW(root.as_ptr()) };
        match t {
            DRIVE_REMOVABLE => out.push(letter),
            DRIVE_FIXED => {
                // Disque USB externe vu comme "fixe" : on vérifie le type de bus.
                if let Ok(h) = open_device(&format!("\\\\.\\{}:", letter), 0) {
                    if let Some(d) = query_descriptor(&h) {
                        if d.bus_type == 7 {
                            out.push(letter);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

struct Descriptor {
    vendor: String,
    product: String,
    revision: String,
    serial: String,
    bus_type: i32,
    removable: bool,
}

fn query_descriptor(h: &Handle) -> Option<Descriptor> {
    let q: STORAGE_PROPERTY_QUERY = unsafe { zeroed() }; // StorageDeviceProperty + StandardQuery = 0
    let mut buf = vec![0u8; 2048];
    unsafe {
        ioctl_raw(
            h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &q as *const _ as *const c_void,
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32,
        )
        .ok()?;
    }
    let d: STORAGE_DEVICE_DESCRIPTOR =
        unsafe { read_unaligned(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
    Some(Descriptor {
        vendor: cstr_at(&buf, d.VendorIdOffset),
        product: cstr_at(&buf, d.ProductIdOffset),
        revision: cstr_at(&buf, d.ProductRevisionOffset),
        serial: cstr_at(&buf, d.SerialNumberOffset),
        bus_type: d.BusType as i32,
        removable: d.RemovableMedia != 0,
    })
}

pub fn disk_number_of(h: &Handle) -> Option<u32> {
    let mut n: STORAGE_DEVICE_NUMBER = unsafe { zeroed() };
    unsafe {
        ioctl_raw(
            h,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            std::ptr::null(),
            0,
            &mut n as *mut _ as *mut c_void,
            size_of::<STORAGE_DEVICE_NUMBER>() as u32,
        )
        .ok()?;
    }
    Some(n.DeviceNumber)
}

/// Numéro de disque physique associé à une lettre de lecteur.
pub fn disk_number(letter: char) -> Option<u32> {
    let h = open_device(&format!("\\\\.\\{}:", letter), 0).ok()?;
    disk_number_of(&h)
}

/// Attribut "lecture seule" au niveau du disque physique.
pub fn disk_readonly(disk: u32) -> Option<bool> {
    let h = open_device(&format!("\\\\.\\PhysicalDrive{}", disk), 0).ok()?;
    let mut a: GET_DISK_ATTRIBUTES = unsafe { zeroed() };
    unsafe {
        ioctl_raw(
            &h,
            IOCTL_DISK_GET_DISK_ATTRIBUTES,
            std::ptr::null(),
            0,
            &mut a as *mut _ as *mut c_void,
            size_of::<GET_DISK_ATTRIBUTES>() as u32,
        )
        .ok()?;
    }
    Some(a.Attributes & 1 != 0)
}

/// Collecte toutes les informations disponibles sur un lecteur.
pub fn gather(letter: char) -> DriveInfo {
    let mut info = DriveInfo { letter, ..Default::default() };
    let root = wide(&info.root());

    info.drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };

    // --- Volume
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let (mut serial, mut maxc, mut flags) = (0u32, 0u32, 0u32);
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut maxc,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    info.has_media = ok != 0;
    if info.has_media {
        info.label = from_wide(&label);
        info.fs = from_wide(&fs);
        info.serial = serial;
        info.max_component = maxc;
        info.fs_flags = flags;

        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut avail, &mut total, &mut free) };
        info.total = total;
        info.free = free;

        let (mut spc, mut bps, mut fc, mut tc) = (0u32, 0u32, 0u32, 0u32);
        if unsafe { GetDiskFreeSpaceW(root.as_ptr(), &mut spc, &mut bps, &mut fc, &mut tc) } != 0 {
            info.cluster_bytes = spc * bps;
            info.sector_bytes = bps;
        }

        let mut guid = [0u16; 64];
        if unsafe { GetVolumeNameForVolumeMountPointW(root.as_ptr(), guid.as_mut_ptr(), 64) } != 0 {
            info.volume_guid = from_wide(&guid);
        }
    }

    // --- Périphérique (via le volume)
    if let Ok(h) = open_device(&format!("\\\\.\\{}:", letter), 0) {
        if let Some(d) = query_descriptor(&h) {
            info.vendor = d.vendor;
            info.product = d.product;
            info.revision = d.revision;
            info.device_serial = d.serial;
            info.bus_type = d.bus_type;
            info.removable_media = d.removable;
        }
        info.disk_number = disk_number_of(&h);

        let mut p: PARTITION_INFORMATION_EX = unsafe { zeroed() };
        let r = unsafe {
            ioctl_raw(
                &h,
                IOCTL_DISK_GET_PARTITION_INFO_EX,
                std::ptr::null(),
                0,
                &mut p as *mut _ as *mut c_void,
                size_of::<PARTITION_INFORMATION_EX>() as u32,
            )
        };
        if r.is_ok() {
            info.part_style = Some(p.PartitionStyle as i32);
            info.part_offset = Some(p.StartingOffset);
            info.part_length = Some(p.PartitionLength);
            info.part_number = Some(p.PartitionNumber);
        }
    }

    // --- Disque physique
    if let Some(n) = info.disk_number {
        info.disk_readonly = disk_readonly(n);
        if let Ok(h) = open_device(&format!("\\\\.\\PhysicalDrive{}", n), GENERIC_READ) {
            let mut len: GET_LENGTH_INFORMATION = unsafe { zeroed() };
            let r = unsafe {
                ioctl_raw(
                    &h,
                    IOCTL_DISK_GET_LENGTH_INFO,
                    std::ptr::null(),
                    0,
                    &mut len as *mut _ as *mut c_void,
                    size_of::<GET_LENGTH_INFORMATION>() as u32,
                )
            };
            if r.is_ok() {
                info.disk_size = Some(len.Length as u64);
            }
        }
    }
    let _ = null_mut::<c_void>();
    info
}

/// Résumé sur une ligne pour les listes de sélection.
pub fn summary(i: &DriveInfo) -> String {
    let name = format!("{} {}", i.vendor, i.product).trim().to_string();
    let name = if name.is_empty() { "Périphérique inconnu".to_string() } else { name };
    if !i.has_media {
        return format!("[{}:] {} — (aucun média)", i.letter, name);
    }
    let ro = if i.volume_readonly() || i.disk_readonly == Some(true) { " 🔒 LECTURE SEULE" } else { "" };
    format!(
        "[{}:] {} — « {} » — {} — {}{}",
        i.letter,
        name,
        i.label,
        i.fs,
        human(i.total),
        ro
    )
}

/// Affiche toutes les informations collectées.
pub fn print_info(i: &DriveInfo) {
    println!();
    println!("════════ Lecteur {}: ════════", i.letter);
    println!("── Périphérique physique ──");
    println!("  Fabricant (VendorId)    : {}", or_dash(&i.vendor));
    println!("  Produit (ProductId)     : {}", or_dash(&i.product));
    println!("  Révision firmware       : {}", or_dash(&i.revision));
    println!("  N° de série matériel    : {}", or_dash(&i.device_serial));
    println!("  Type de bus             : {}", bus_name(i.bus_type));
    println!("  Type de lecteur Windows : {}", drive_type_name(i.drive_type));
    println!("  Média amovible          : {}", yes_no(i.removable_media));
    if let Some(n) = i.disk_number {
        println!("  Disque physique         : \\\\.\\PhysicalDrive{}", n);
    }
    if let Some(s) = i.disk_size {
        println!("  Capacité réelle du disque: {} ({} octets)", human(s), s);
    }
    match i.disk_readonly {
        Some(true) => println!("  Attribut disque READ-ONLY: OUI  ← bloqué au niveau du disque"),
        Some(false) => println!("  Attribut disque READ-ONLY: non"),
        None => println!("  Attribut disque READ-ONLY: indisponible"),
    }

    if let Some(style) = i.part_style {
        println!("── Partition ──");
        println!("  Table de partitions     : {}", part_style_name(style));
        if let Some(n) = i.part_number {
            println!("  N° de partition         : {}", n);
        }
        if let Some(o) = i.part_offset {
            let aligned = o % (1024 * 1024) == 0;
            println!(
                "  Offset de départ        : {} octets ({})",
                o,
                if aligned { "aligné sur 1 Mio" } else { "NON aligné sur 1 Mio" }
            );
        }
        if let (Some(l), Some(d)) = (i.part_length, i.disk_size) {
            let unused = d.saturating_sub(l as u64);
            println!("  Taille de la partition  : {}", human(l as u64));
            if unused > 4 * 1024 * 1024 {
                println!("  Espace hors partition   : {} (non utilisé)", human(unused));
            }
        }
    }

    println!("── Volume / système de fichiers ──");
    if !i.has_media {
        println!("  Aucun média ou système de fichiers illisible (RAW ?).");
        return;
    }
    println!("  Nom du volume           : {}", or_dash(&i.label));
    println!("  Système de fichiers     : {}", or_dash(&i.fs));
    println!("  N° de série du volume   : {:04X}-{:04X}", i.serial >> 16, i.serial & 0xFFFF);
    println!("  GUID du volume          : {}", or_dash(&i.volume_guid));
    println!("  Taille totale           : {}", human(i.total));
    println!(
        "  Espace libre            : {} ({:.1} %)",
        human(i.free),
        if i.total > 0 { i.free as f64 * 100.0 / i.total as f64 } else { 0.0 }
    );
    println!("  Espace utilisé          : {}", human(i.total.saturating_sub(i.free)));
    println!("  Taille de cluster       : {} octets", i.cluster_bytes);
    println!("  Taille de secteur       : {} octets", i.sector_bytes);
    println!("  Longueur max. d'un nom  : {} caractères", i.max_component);
    println!(
        "  Volume en lecture seule : {}",
        if i.volume_readonly() { "OUI  ← bloqué au niveau du volume" } else { "non" }
    );
    let feats: [(u32, &str); 8] = [
        (FILE_PERSISTENT_ACLS, "ACL"),
        (FILE_FILE_COMPRESSION, "compression"),
        (FILE_SUPPORTS_ENCRYPTION, "chiffrement EFS"),
        (FILE_SUPPORTS_SPARSE_FILES, "fichiers creux"),
        (FILE_SUPPORTS_HARD_LINKS, "liens physiques"),
        (FILE_SUPPORTS_REPARSE_POINTS, "points de reparse"),
        (FILE_NAMED_STREAMS, "flux nommés"),
        (FILE_CASE_SENSITIVE_SEARCH, "recherche sensible à la casse"),
    ];
    let list: Vec<&str> = feats.iter().filter(|(f, _)| i.fs_flags & f != 0).map(|(_, n)| *n).collect();
    println!("  Fonctions du FS         : {}", if list.is_empty() { "-".into() } else { list.join(", ") });
    println!("  Flags bruts             : 0x{:08X}", i.fs_flags);
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}
fn yes_no(b: bool) -> &'static str {
    if b { "oui" } else { "non" }
}
