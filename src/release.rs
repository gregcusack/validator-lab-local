use {
    crate::{boxed_error, SOLANA_ROOT},
    git2::Repository,
    log::*,
    std::{error::Error, fmt::Display, path::PathBuf, str::FromStr, time::Instant},
};

#[derive(Debug)]
pub enum DeployMethod {
    Local,
    Tar,
    Skip,
}

impl Display for DeployMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployMethod::Local => write!(f, "local"),
            DeployMethod::Tar => write!(f, "tar"),
            DeployMethod::Skip => write!(f, "skip"),
        }
    }
}

impl FromStr for DeployMethod {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(DeployMethod::Local),
            "tar" => Ok(DeployMethod::Tar),
            "skip" => Ok(DeployMethod::Skip),
            _ => Err(()),
        }
    }
}

pub struct BuildConfig {
    deploy_method: DeployMethod,
    do_build: bool,
    debug_build: bool,
    _build_path: PathBuf,
    local_repo_path: Option<PathBuf>, 
    solana_root_path: PathBuf,
}

impl BuildConfig {
    pub fn new(
        deploy_method: &str,
        do_build: bool,
        debug_build: bool,
        local_repo_path: Option<PathBuf>,
        solana_root_path: &PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let deploy_method = deploy_method
            .parse::<DeployMethod>()
            .map_err(|_| "Failed to parse deploy_method".to_string())?;

        let build_path = match deploy_method {
            DeployMethod::Local => SOLANA_ROOT.join("farf/bin"),
            DeployMethod::Tar => SOLANA_ROOT.join("solana-release/bin"),
            DeployMethod::Skip => SOLANA_ROOT.join("farf/bin"),
        };

        Ok(BuildConfig {
            deploy_method,
            do_build,
            debug_build,
            _build_path: build_path,
            local_repo_path,
            solana_root_path: solana_root_path.clone(),
        })
    }

    pub async fn prepare(&self) -> Result<(), Box<dyn Error>> {
        match self.deploy_method {
            DeployMethod::Tar => {
                return Err(boxed_error!("Tar deploy method not implemented yet."));
            }
            DeployMethod::Local => {
                match self.setup_local_deploy() {
                    Ok(_) => (),
                    Err(err) => return Err(err),
                };
            }
            DeployMethod::Skip => {
                return Err(boxed_error!("Skip deploy method not implemented yet."));
            }
        }
        info!("Completed Prepare Deploy");
        Ok(())
    }

    fn setup_local_deploy(&self) -> Result<(), Box<dyn Error>> {
        if self.do_build {
            self.build()?;
        } else {
            info!("Build skipped due to --no-build");
        }
        Ok(())
    }

    fn build(&self) -> Result<(), Box<dyn Error>> {
        let start_time = Instant::now();
        let build_variant = if self.debug_build { "--debug" } else { "" };
        let build_repo = match &self.local_repo_path {
            Some(repo) => repo.to_str().unwrap(),
            None => ""
        };
        info!("build path: {}", build_repo);

        let install_directory = self.solana_root_path.join("farf");
        let install_script = self.solana_root_path.join("scripts/cargo-install-all.sh");
        match std::process::Command::new(install_script)
            .arg(install_directory)
            .arg(build_variant)
            .arg("--validator-only")
            .status()
        {
            Ok(result) => {
                if result.success() {
                    info!("Successfully build validator")
                } else {
                    return Err(boxed_error!("Failed to build validator"));
                }
            }
            Err(err) => {
                info!("here1");
                return Err(Box::new(err))
            }
        }

        // match std::process::Command::new("./cargo-install-all.sh")
        //     .current_dir(SOLANA_ROOT.join("scripts"))
        //     .arg(install_directory)
        //     .arg(build_variant)
        //     .arg("--validator-only")
        //     // .arg(build_repo)
        //     .status()
        // {
        //     Ok(result) => {
        //         if result.success() {
        //             info!("Successfully build validator")
        //         } else {
        //             return Err(boxed_error!("Failed to build validator"));
        //         }
        //     }
        //     Err(err) => {
        //         info!("here1");
        //         return Err(Box::new(err))
        //     }
        // }

        // let solana_repo = Repository::open(SOLANA_ROOT.as_path())?;
        let solana_repo = Repository::open(self.solana_root_path.as_path())?;
        let commit = solana_repo.revparse_single("HEAD")?.id();
        let branch = solana_repo
            .head()?
            .shorthand()
            .expect("Failed to get shortened branch name")
            .to_string();

        // Check if current commit is associated with a tag
        let mut note = branch;
        for tag in (&solana_repo.tag_names(None)?).into_iter().flatten() {
            // Get the target object of the tag
            let tag_object = solana_repo.revparse_single(tag)?.id();
            // Check if the commit associated with the tag is the same as the current commit
            if tag_object == commit {
                info!("The current commit is associated with tag: {}", tag);
                note = tag_object.to_string();
                break;
            }
        }

        // Write to branch/tag and commit to version.yml
        let content = format!("channel: devbuild {}\ncommit: {}", note, commit);
        std::fs::write(self.solana_root_path.join("farf/version.yml"), content)
            .expect("Failed to write version.yml");

        info!("Build took {:.3?} seconds", start_time.elapsed());
        Ok(())
    }
}
