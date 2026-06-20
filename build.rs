fn main() {
    if cfg!(windows) {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon-256.ico");
        // Ne pas faire échouer la compilation si le compilateur de ressources
        // (rc.exe / windres) est indisponible : l'icône intégrée à l'exe est
        // cosmétique (l'icône de la barre des tâches est chargée à l'exécution).
        if let Err(e) = res.compile() {
            println!("cargo:warning=Intégration de l'icône Windows ignorée : {e}");
        }
    }
}
