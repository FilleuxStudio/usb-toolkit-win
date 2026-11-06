//! Interface console interactive (menus en français).

use std::io::{self, Write};
use std::path::PathBuf;

use windows_sys::Win32::System::Console::SetConsoleOutputCP;
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;

use crate::drives::{self, human};
use crate::ops;

fn ask(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s).ok();
    s.trim().trim_matches('"').to_string()
}

fn confirm(prompt: &str) -> bool {
    matches!(ask(&format!("{} (o/N) : ", prompt)).to_lowercase().as_str(), "o" | "oui" | "y" | "yes")
}

fn pause() {
    ask("\nAppuyez sur ENTRÉE pour continuer…");
}

fn choose_drive() -> Option<char> {
    let list = drives::list_usb_drives();
    if list.is_empty() {
        println!("\nAucune clé USB détectée. Branchez-en une puis réessayez.");
        return None;
    }
    println!("\nLecteurs USB détectés :");
    for l in &list {
        println!("  {}", drives::summary(&drives::gather(*l)));
    }
    let a = ask("\nLettre du lecteur (ENTRÉE pour annuler) : ");
    let c = a.trim_end_matches(':').chars().next()?.to_ascii_uppercase();
    if list.contains(&c) {
        Some(c)
    } else {
        println!("Lecteur invalide ou non-USB : par sécurité l'outil refuse d'y toucher.");
        None
    }
}

pub fn run() {
    unsafe { SetConsoleOutputCP(65001) };
    loop {
        println!("\n╔════════════════════════════════════════════════╗");
        println!("║        USB TOOLKIT WIN — gestion de clés USB   ║");
        println!("╚════════════════════════════════════════════════╝");
        if unsafe { IsUserAnAdmin() } == 0 {
            println!("⚠  Pas en administrateur : la plupart des actions échoueront.");
        }
        println!(" 1. Lister les clés + infos détaillées");
        println!(" 2. Débloquer une clé en lecture seule");
        println!(" 3. Renommer une clé");
        println!(" 4. Formater / changer le système de fichiers");
        println!(" 5. Convertir FAT32 → NTFS sans perte de données");
        println!(" 6. Fichiers cachés (créer, masquer, afficher, récupérer)");
        println!(" 7. Icône du lecteur");
        println!(" 8. Infos avancées (WMI : VID/PID, firmware, secteurs…)");
        println!(" 0. Quitter");
        match ask("\nVotre choix : ").as_str() {
            "1" => menu_info(),
            "2" => menu_unlock(),
            "3" => menu_rename(),
            "4" => menu_format(),
            "5" => menu_convert(),
            "6" => menu_hidden(),
            "7" => menu_icon(),
            "8" => menu_advanced(),
            "0" | "q" => break,
            _ => println!("Choix invalide."),
        }
    }
}

fn menu_info() {
    let list = drives::list_usb_drives();
    if list.is_empty() {
        println!("\nAucune clé USB détectée.");
    }
    for l in list {
        drives::print_info(&drives::gather(l));
    }
    pause();
}

fn menu_unlock() {
    let Some(l) = choose_drive() else { return };
    let i = drives::gather(l);
    println!(
        "\nÉtat actuel : volume read-only = {}, disque read-only = {:?}",
        i.volume_readonly(),
        i.disk_readonly
    );
    if confirm(&format!("Tenter de débloquer {}: ?", l)) {
        ops::unlock(l);
    }
    pause();
}

fn menu_rename() {
    let Some(l) = choose_drive() else { return };
    let i = drives::gather(l);
    if !i.has_media {
        println!("Pas de média.");
        return;
    }
    println!("Nom actuel : « {} » ({})", i.label, i.fs);
    let n = ask("Nouveau nom (vide = aucun nom) : ");
    match ops::rename(l, &n, &i.fs) {
        Ok(()) => println!("✔ Renommé."),
        Err(e) => println!("✘ {}", e),
    }
    pause();
}

fn menu_format() {
    let Some(l) = choose_drive() else { return };
    let i = drives::gather(l);
    println!("\nActuellement : {}", drives::summary(&i));
    println!("\nNouveau système de fichiers :");
    println!("  1. exFAT  (recommandé : Windows/Mac/Linux, fichiers > 4 Go)");
    println!("  2. NTFS   (Windows, permissions, fichiers > 4 Go)");
    println!("  3. FAT32  (compatibilité maximale, max 32 Go via Windows, fichiers < 4 Go)");
    println!("  4. FAT    (très ancien matériel)");
    let fs = match ask("Choix : ").as_str() {
        "1" => "exFAT",
        "2" => "NTFS",
        "3" => "FAT32",
        "4" => "FAT",
        _ => return,
    };
    let label = ask("Nom du volume (peut rester vide) : ");
    let quick = confirm("Formatage rapide ? (non = complet, beaucoup plus long)");
    println!("\n⚠  TOUTES LES DONNÉES de {}: ({}) vont être EFFACÉES.", l, human(i.total));
    let typed = ask(&format!("Tapez la lettre « {} » pour confirmer : ", l));
    if !typed.eq_ignore_ascii_case(&l.to_string()) {
        println!("Annulé.");
        return;
    }
    match ops::format_drive(l, fs, &label, quick) {
        Ok(()) => println!("\n✔ Formatage terminé."),
        Err(e) => println!("\n✘ {}", e),
    }
    pause();
}

fn menu_convert() {
    let Some(l) = choose_drive() else { return };
    let i = drives::gather(l);
    if !i.fs.to_uppercase().starts_with("FAT") {
        println!("La clé est en {} : la conversion ne concerne que FAT/FAT32.", i.fs);
        return;
    }
    println!("Conversion irréversible (NTFS → FAT32 impossible sans reformater). Sauvegardez vos données.");
    if confirm("Continuer ?") {
        match ops::convert_to_ntfs(l) {
            Ok(()) => println!("✔ Conversion terminée."),
            Err(e) => println!("✘ {}", e),
        }
    }
    pause();
}

fn menu_hidden() {
    let Some(l) = choose_drive() else { return };
    loop {
        println!("\n── Fichiers cachés sur {}: ──", l);
        println!(" 1. Créer un fichier caché");
        println!(" 2. Créer un dossier caché");
        println!(" 3. Masquer un fichier/dossier existant");
        println!(" 4. Rendre visible un fichier/dossier");
        println!(" 5. Lister tous les éléments cachés/système");
        println!(" 6. Tout rendre visible (récupération après « virus raccourci »)");
        println!(" 0. Retour");
        match ask("Choix : ").as_str() {
            "1" => {
                let n = ask("Nom du fichier (ex: secret.txt ou dossier\\note.txt) : ");
                let c = ask("Contenu texte : ");
                let sys = confirm("Ajouter aussi l'attribut Système (encore plus discret) ?");
                match ops::create_hidden_file(l, &n, &c, sys) {
                    Ok(p) => println!("✔ Créé : {}", p.display()),
                    Err(e) => println!("✘ {}", e),
                }
            }
            "2" => {
                let n = ask("Nom du dossier : ");
                let sys = confirm("Ajouter aussi l'attribut Système ?");
                match ops::create_hidden_dir(l, &n, sys) {
                    Ok(p) => println!("✔ Créé : {}", p.display()),
                    Err(e) => println!("✘ {}", e),
                }
            }
            c @ ("3" | "4") => {
                let rel = ask("Chemin relatif à la racine (ex: Photos ou docs\\rapport.pdf) : ");
                match ops::safe_path(l, &rel) {
                    Ok(p) => {
                        let r = if c == "3" {
                            let sys = confirm("Ajouter aussi l'attribut Système ?");
                            ops::hide(&p, sys)
                        } else {
                            ops::unhide(&p)
                        };
                        match r {
                            Ok(()) => println!("✔ Fait."),
                            Err(e) => println!("✘ {}", e),
                        }
                    }
                    Err(e) => println!("✘ {}", e),
                }
            }
            "5" => {
                let v = ops::scan_hidden(l);
                if v.is_empty() {
                    println!("Aucun élément caché.");
                }
                for p in &v {
                    println!("  {}", p.display());
                }
            }
            "6" => {
                let v = ops::scan_hidden(l);
                println!("{} élément(s) caché(s) trouvé(s).", v.len());
                if !v.is_empty() && confirm("Retirer les attributs Caché/Système de tous ?") {
                    let mut ok = 0;
                    for p in &v {
                        if ops::unhide(p).is_ok() {
                            ok += 1;
                        }
                    }
                    println!("✔ {} élément(s) rendu(s) visible(s).", ok);
                }
            }
            "0" => return,
            _ => println!("Choix invalide."),
        }
    }
}

fn menu_icon() {
    let Some(l) = choose_drive() else { return };
    println!("\n 1. Utiliser l'icône intégrée de USB Toolkit");
    println!(" 2. Choisir mon propre fichier .ico");
    println!(" 3. Supprimer l'icône personnalisée");
    let choice = ask("Choix : ");
    match choice.as_str() {
        "1" | "2" => {
            let path = if choice == "2" { Some(ask("Chemin du fichier .ico : ")) } else { None };
            let name = ask("Nom affiché dans l'Explorateur (ASCII, vide = inchangé) : ");
            let label = if name.is_empty() { None } else { Some(name.as_str()) };
            let r = match path {
                Some(p) => ops::set_drive_icon_from_file(l, &PathBuf::from(p), label),
                None => ops::set_drive_icon(l, ops::BUILTIN_ICON, label),
            };
            match r {
                Ok(()) => println!("✔ Icône appliquée (débranchez/rebranchez la clé si elle n'apparaît pas)."),
                Err(e) => println!("✘ {}", e),
            }
        }
        "3" => match ops::remove_drive_icon(l) {
            Ok(()) => println!("✔ Icône supprimée."),
            Err(e) => println!("✘ {}", e),
        },
        _ => {}
    }
    pause();
}

fn menu_advanced() {
    let Some(l) = choose_drive() else { return };
    match drives::disk_number(l) {
        Some(n) => ops::advanced_info(n),
        None => println!("Numéro de disque introuvable."),
    }
    pause();
}
