use std::env;
use std::io;

fn main() {
    // L'intégration de l'icône au fichier .exe est cosmétique (l'icône de la
    // barre des tâches est chargée à l'exécution) : on avertit au lieu d'échouer
    // si le compilateur de ressources est indisponible.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let result = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        // winres écrit le chemin de l'icône avec des backslashes dans le .rc,
        // que le compilateur de ressources GNU (windres) interprète comme des
        // séquences d'échappement (« \U », « \A »…) et rejette. On compile donc
        // un .rc minimal référençant l'icône par son nom de base (sans backslash).
        embed_icon_gnu()
    } else {
        // msvc : rc.exe gère les chemins Windows, winres fonctionne tel quel.
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon-256.ico");
        res.compile()
    };
    if let Err(e) = result {
        println!("cargo:warning=Intégration de l'icône Windows ignorée : {e}");
    }
}

fn embed_icon_gnu() -> io::Result<()> {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR défini par cargo");
    println!("cargo:rerun-if-changed=assets/icon-256.ico");
    std::fs::copy("assets/icon-256.ico", format!("{out_dir}/icon-256.ico"))?;
    std::fs::write(format!("{out_dir}/icon.rc"), "1 ICON \"icon-256.ico\"\n")?;
    let windres = env::var("WINDRES").unwrap_or_else(|_| "windres".to_string());
    let status = std::process::Command::new(windres)
        .current_dir(&out_dir)
        .args(["icon.rc", "-O", "coff", "-o", "icon.o"])
        .status()?;
    if !status.success() {
        return Err(io::Error::other("windres a échoué"));
    }
    println!("cargo:rustc-link-arg-bins={out_dir}/icon.o");
    Ok(())
}
