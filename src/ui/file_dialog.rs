/// Subprocess-based file picker/saver fallback for when the xdg-desktop-portal
/// is not available. Tries zenity (GTK) then kdialog (Qt/KDE).

pub enum DialogMode {
    Open,
    Save,
}

/// `ext` is the extension to default to when saving (no leading dot, e.g. "fountain").
/// `filter_label` is the human-readable filter name shown in the picker, e.g.
/// "Fountain Files (*.fountain)".
pub async fn pick_file_subprocess(
    mode: DialogMode, ext: &str, filter_label: &str,
) -> Option<std::path::PathBuf> {
    let (tx, rx) = futures::channel::oneshot::channel::<Option<std::path::PathBuf>>();
    let ext = ext.to_string();
    let filter_label = filter_label.to_string();
    std::thread::spawn(move || {
        let is_save = matches!(mode, DialogMode::Save);
        let glob = format!("*.{ext}");

        // Try zenity (GTK / most desktop environments)
        let mut zenity_args: Vec<String> = vec!["--file-selection".to_string()];
        if is_save { zenity_args.push("--save".to_string()); }
        zenity_args.extend([
            if is_save { "--title=Save File".to_string() } else { "--title=Open File".to_string() },
            format!("--file-filter={filter_label}|{glob}"),
            "--file-filter=All Files|*".to_string(),
        ]);
        let zenity = std::process::Command::new("zenity").args(&zenity_args).output();
        if let Ok(out) = zenity {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    let mut path = std::path::PathBuf::from(s);
                    if is_save && path.extension().is_none() {
                        path.set_extension(&ext);
                    }
                    tx.send(Some(path)).ok();
                    return;
                }
            }
            // exit 1 = user cancelled → send None and stop; any other code = not installed
            if out.status.code() == Some(1) {
                tx.send(None).ok();
                return;
            }
        }

        // Try kdialog (KDE / Qt environments)
        let kdialog = std::process::Command::new("kdialog")
            .args([
                if is_save { "--getsavefilename" } else { "--getopenfilename" },
                ".",
                &format!("{glob}|{filter_label}\n*.*|All Files"),
            ])
            .output();
        if let Ok(out) = kdialog {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    let mut path = std::path::PathBuf::from(s);
                    if is_save && path.extension().is_none() {
                        path.set_extension(&ext);
                    }
                    tx.send(Some(path)).ok();
                    return;
                }
            }
        }

        eprintln!("no file picker available (install zenity or kdialog)");
        tx.send(None).ok();
    });

    rx.await.ok().flatten()
}
