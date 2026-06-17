/// Subprocess-based file picker/saver fallback for when the xdg-desktop-portal
/// is not available. Tries zenity (GTK) then kdialog (Qt/KDE).

pub enum DialogMode {
    Open,
    Save,
}

pub async fn pick_file_subprocess(mode: DialogMode) -> Option<std::path::PathBuf> {
    let (tx, rx) = futures::channel::oneshot::channel::<Option<std::path::PathBuf>>();
    std::thread::spawn(move || {
        let is_save = matches!(mode, DialogMode::Save);

        // Try zenity (GTK / most desktop environments)
        let mut zenity_args: Vec<&str> = vec!["--file-selection"];
        if is_save { zenity_args.push("--save"); }
        zenity_args.extend_from_slice(&[
            if is_save { "--title=Save Fountain Script" } else { "--title=Open Fountain Script" },
            "--file-filter=Fountain Files (*.fountain)|*.fountain",
            "--file-filter=All Files|*",
        ]);
        let zenity = std::process::Command::new("zenity").args(&zenity_args).output();
        if let Ok(out) = zenity {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    let mut path = std::path::PathBuf::from(s);
                    if is_save && path.extension().is_none() {
                        path.set_extension("fountain");
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
                "*.fountain|Fountain Files\n*.*|All Files",
            ])
            .output();
        if let Ok(out) = kdialog {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    let mut path = std::path::PathBuf::from(s);
                    if is_save && path.extension().is_none() {
                        path.set_extension("fountain");
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
