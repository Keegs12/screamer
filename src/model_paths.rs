use std::path::PathBuf;

pub(crate) fn find_model(model_name: &str) -> Option<PathBuf> {
    let candidates = [
        format!("ggml-{}.en.bin", model_name),
        format!("ggml-{}.bin", model_name),
        format!("ggml-{}-v3.bin", model_name),
    ];

    let bundle_models_dir = current_bundle_models_dir();

    for filename in &candidates {
        if let Some(ref dir) = bundle_models_dir {
            let path = dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }

        let local = PathBuf::from("models").join(filename);
        if local.exists() {
            return Some(local);
        }
    }

    None
}

fn current_bundle_models_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|exe| {
        exe.parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("Resources").join("models"))
    })
}
