//! Desktop integration for the Linux AppImage.
//!
//! An AppImage is a single file: the desktop knows nothing about it, so the
//! dock, the app grid and (on Wayland) the window itself show a generic icon.
//! When Ferrowl runs from an AppImage it installs a menu entry and its icons
//! in the user's data directory, pointing at the AppImage's current path.
//! The .deb and tarball installs are left alone.

use std::fs;
use std::path::{Path, PathBuf};

/// Marks entries written by Ferrowl, so a user's own entry is never replaced.
const MARKER: &str = "X-Ferrowl-AppImage=true";

const ICONS: &[(&str, &[u8])] = &[
    ("16x16", include_bytes!("../assets/icons/png/ferrowl-16.png")),
    ("32x32", include_bytes!("../assets/icons/png/ferrowl-32.png")),
    ("48x48", include_bytes!("../assets/icons/png/ferrowl-48.png")),
    ("64x64", include_bytes!("../assets/icons/png/ferrowl-64.png")),
    ("128x128", include_bytes!("../assets/icons/png/ferrowl-128.png")),
    ("256x256", include_bytes!("../assets/icons/png/ferrowl-256.png")),
    ("512x512", include_bytes!("../assets/icons/png/ferrowl-512.png")),
];

/// Installs or refreshes the menu entry when running from an AppImage.
/// Errors are ignored: integration is a convenience, never a reason to fail.
pub fn integrate_appimage() {
    let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
        return;
    };
    if !appimage.is_file() {
        return;
    }
    let Some(data) = dirs::data_dir() else {
        return;
    };
    let _ = install(&data, &appimage);
    set_file_icon(&data, &appimage);
}

/// File managers show an AppImage with a generic icon. GNOME Files (and
/// other GVfs-based ones) honour a per-file custom icon: point it at the
/// installed owl so the downloaded file looks like Ferrowl after first run.
fn set_file_icon(data: &Path, appimage: &Path) {
    let icon = data.join("icons/hicolor/256x256/apps/ferrowl.png");
    if !icon.is_file() {
        return;
    }
    let uri = format!("file://{}", icon.display());
    let _ = std::process::Command::new("gio")
        .arg("set")
        .arg(appimage)
        .arg("metadata::custom-icon")
        .arg(uri)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn install(data: &Path, appimage: &Path) -> std::io::Result<()> {
    let entry_path = data.join("applications").join("ferrowl.desktop");
    let wanted = desktop_entry(appimage);
    match fs::read_to_string(&entry_path) {
        Ok(current) if current == wanted => return Ok(()),
        // Someone else's entry (or a hand-edited one): keep it.
        Ok(current) if !current.contains(MARKER) => return Ok(()),
        _ => {}
    }

    for (size, png) in ICONS {
        let dir = data.join("icons/hicolor").join(size).join("apps");
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("ferrowl.png"), png)?;
    }

    fs::create_dir_all(entry_path.parent().unwrap_or(data))?;
    let tmp = entry_path.with_extension("desktop.tmp");
    fs::write(&tmp, wanted)?;
    fs::rename(&tmp, &entry_path)?;

    // Optional: refreshes caches on desktops that use them.
    let _ = std::process::Command::new("update-desktop-database")
        .arg(data.join("applications"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    Ok(())
}

fn desktop_entry(appimage: &Path) -> String {
    let exec = quote_exec(&appimage.to_string_lossy());
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Ferrowl\n\
         GenericName=Email client\n\
         GenericName[it]=Client di posta\n\
         Comment=Fast, private email client\n\
         Comment[it]=Client di posta veloce e riservato\n\
         Exec={exec} %U\n\
         TryExec={try_exec}\n\
         Icon=ferrowl\n\
         Terminal=false\n\
         Categories=Network;Email;\n\
         Keywords=mail;email;imap;smtp;openpgp;\n\
         StartupWMClass=ferrowl\n\
         {MARKER}\n",
        try_exec = escape_value(&appimage.to_string_lossy()),
    )
}

/// Quotes a path for the Exec key (Desktop Entry spec, "The Exec key").
fn quote_exec(path: &str) -> String {
    let mut out = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    escape_value(&out)
}

/// Escapes a string value for a .desktop file.
fn escape_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_quotes_spaces_and_specials() {
        assert_eq!(quote_exec("/home/a b/F.AppImage"), "\"/home/a b/F.AppImage\"");
        assert_eq!(quote_exec("/x/100%/$y"), "\"/x/100%%/\\\\$y\"");
    }

    #[test]
    fn installs_then_respects_foreign_entries() {
        let data = std::env::temp_dir().join(format!("ferrowl-de-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        let app = Path::new("/opt/apps/Ferrowl-x86_64.AppImage");

        install(&data, app).unwrap();
        let entry = data.join("applications/ferrowl.desktop");
        let text = fs::read_to_string(&entry).unwrap();
        assert!(text.contains("Exec=\"/opt/apps/Ferrowl-x86_64.AppImage\" %U"));
        assert!(data.join("icons/hicolor/256x256/apps/ferrowl.png").is_file());

        // Moved AppImage: entry follows it.
        install(&data, Path::new("/new/Ferrowl.AppImage")).unwrap();
        assert!(
            fs::read_to_string(&entry)
                .unwrap()
                .contains("/new/Ferrowl.AppImage")
        );

        // A user's own entry is never touched.
        fs::write(&entry, "[Desktop Entry]\nName=Mine\n").unwrap();
        install(&data, app).unwrap();
        assert_eq!(
            fs::read_to_string(&entry).unwrap(),
            "[Desktop Entry]\nName=Mine\n"
        );

        fs::remove_dir_all(&data).unwrap();
    }
}
