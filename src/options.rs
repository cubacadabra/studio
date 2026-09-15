use super::*;

pub(crate) struct StudioOptions {
    pub(crate) game_path: Option<PathBuf>,
    pub(crate) validate_project_path: Option<PathBuf>,
    pub(crate) morph_catalog_path: Option<PathBuf>,
}

pub(crate) fn parse_options() -> Result<StudioOptions, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let mut game_path = None;
    let mut validate_project_path = None;
    let mut morph_catalog_path = None;
    while let Some(argument) = args.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Usage: studio [--path <game-directory>] [--validate-project <game-directory>] [--morph-catalog <catalog.json>]"
            );
            println!();
            println!(
                "Open a local Cubacadabra game package. Without --path, Studio opens the project chooser."
            );
            std::process::exit(0);
        }
        if argument == "--path" {
            game_path = Some(
                args.next()
                    .ok_or_else(|| StudioError("--path expects a game directory".to_owned()))?,
            );
        } else if argument == "--validate-project" {
            validate_project_path = Some(args.next().ok_or_else(|| {
                StudioError("--validate-project expects a game directory".to_owned())
            })?);
        } else if argument == "--morph-catalog" {
            morph_catalog_path = Some(args.next().ok_or_else(|| {
                StudioError("--morph-catalog expects a catalog JSON file".to_owned())
            })?);
        } else {
            return Err(Box::new(StudioError(format!(
                "unknown argument: {}",
                argument.to_string_lossy()
            ))));
        }
    }
    let game_path = game_path
        .map(PathBuf::from)
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &game_path
        && !path.is_dir()
    {
        return Err(Box::new(StudioError(format!(
            "game path is not a directory: {}",
            path.display()
        ))));
    }
    let validate_project_path = validate_project_path
        .map(PathBuf::from)
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &validate_project_path
        && !path.is_dir()
    {
        return Err(Box::new(StudioError(format!(
            "project path is not a directory: {}",
            path.display()
        ))));
    }
    let morph_catalog_path = morph_catalog_path
        .map(PathBuf::from)
        .map(|path| path.canonicalize())
        .transpose()?;
    if let Some(path) = &morph_catalog_path
        && !path.is_file()
    {
        return Err(Box::new(StudioError(format!(
            "morph catalog is not a file: {}",
            path.display()
        ))));
    }
    Ok(StudioOptions {
        game_path,
        validate_project_path,
        morph_catalog_path,
    })
}

pub(crate) fn validate_project(game_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let sources = load_game_sources(Some(game_path.clone()))?;
    let client = ClientSession::load(&sources.manifest_source, &sources.script_source)?;
    println!(
        "Validated {} ({}) through the shared game builder.",
        game_path.display(),
        client.game_id()
    );
    Ok(())
}
