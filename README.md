<div align="center">

<img src="assets/icon_preview.png" width="128" alt="USB Toolkit">

# USB Toolkit

**Le couteau suisse gratuit pour dompter vos clés USB sous Windows.**

![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D6?logo=windows&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-%F0%9F%A6%80-orange)
![Licence](https://img.shields.io/badge/licence-Apache%202.0-blue)
![Statut](https://img.shields.io/badge/statut-b%C3%AAta-yellow)

</div>

---

## C'est quoi ?

**USB Toolkit** est un petit outil en ligne de commande (avec un menu simple, en français) qui regroupe au même endroit tout ce qu'on cherche habituellement dans dix menus Windows différents quand une clé USB fait des siennes.

Un seul `.exe`, aucune installation, aucune dépendance. Open source et gratuit.

## À quoi ça sert ?

- Votre clé est **bloquée en « lecture seule »** et Windows dit « le disque est protégé en écriture » ?
- Vous voulez **formater** une clé ou **changer son format** (FAT32, exFAT, NTFS) ?
- Vos fichiers ont **disparu** après un « virus raccourci » alors qu'ils sont juste cachés ?
- Vous voulez **personnaliser** votre clé avec un nom et une icône à vous ?
- Vous voulez savoir **ce que contient vraiment** votre clé : fabricant, numéro de série, VID/PID, vraie capacité… ce que Windows ne vous montre pas ?

## Ce qu'elle sait faire

| | Fonction | Détail |
|---|---|---|
| 🔍 | **Infos détaillées** | Fabricant, produit, firmware, n° de série matériel, type de bus, capacité réelle, MBR/GPT, alignement, taille de cluster et de secteur, GUID du volume, fonctions du système de fichiers |
| 🔓 | **Débloquer la lecture seule** | Essaie le registre, l'attribut du disque, l'attribut du volume, puis fait un **vrai test d'écriture** pour confirmer |
| ✏️ | **Renommer** | Avec contrôle automatique des règles FAT / NTFS |
| 💾 | **Formater** | exFAT, NTFS, FAT32 ou FAT, rapide ou complet, avec confirmation de sécurité |
| 🔄 | **Convertir FAT32 → NTFS** | Sans perdre vos données |
| 🙈 | **Fichiers cachés** | Créer, masquer, afficher, lister, ou tout révéler d'un coup |
| 🎨 | **Icône du lecteur** | Icône intégrée ou votre propre `.ico`, plus un nom affiché personnalisé |
| 🧠 | **Infos avancées** | VID/PID USB, dates d'installation, secteurs physiques, état de santé (via WMI/PowerShell) |

## 🚀 Utilisation

1. Téléchargez `usb-toolkit.exe` (section *Releases*) ou compilez-le vous-même (voir plus bas).
2. Double-cliquez dessus, acceptez la demande d'administrateur (indispensable pour accéder aux disques).
3. Branchez votre clé, choisissez une option dans le menu, et suivez les instructions.

## 🛡️ Sécurité

- Seuls les lecteurs **USB / amovibles** sont proposés : votre disque système n'est **jamais** listé.
- Le formatage demande de **retaper la lettre du lecteur** pour confirmer.
- Les chemins pour les fichiers cachés restent **enfermés dans la clé**.

> ⚠️ **Le formatage et la conversion sont irréversibles.** Sauvegardez avant. L'outil est fourni sans garantie (voir la licence) : vérifiez toujours la lettre du lecteur avant de confirmer.

## ⚠️ Ce qu'elle ne peut pas faire

- Lever un **verrou physique** (petit interrupteur sur la clé, contrôleur défaillant). Beaucoup de clés en fin de vie passent en lecture seule pour protéger vos données : dans ce cas, sauvegardez et remplacez la clé.
- Formater en **FAT32 au-delà de 32 Go** (limite de Windows) : choisissez exFAT ou NTFS.
- Fonctionner sous **Linux ou macOS** : l'outil s'appuie sur les API de Windows.

## 🔧 Compiler soi-même

1. Installez Rust : <https://rustup.rs> et les *Build Tools for Visual Studio* (composant « Développement Desktop en C++ »).
2. Puis :

```
git clone https://github.com/VOTRE-NOM/usb-toolkit
cd usb-toolkit
cargo build --release
```

L'exécutable se trouve dans `target\release\usb-toolkit.exe`. Pour changer l'icône de l'application, remplacez `assets/icon.ico` et recompilez.

## Structure du projet

```
src/main.rs     point d'entrée
src/ui.rs       menus interactifs
src/drives.rs   détection des clés et collecte d'informations
src/ops.rs      déblocage, formatage, renommage, fichiers cachés, icône
src/win.rs      utilitaires Win32
build.rs        icône, infos de version et manifeste administrateur
```


## Licence

Distribué sous licence **Apache 2.0**. Voir le fichier [LICENSE](LICENSE).
