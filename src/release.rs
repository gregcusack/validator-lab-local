use {
    crate::{boxed_error, cat_file, download_to_temp, extract_release_archive},
    git2::Repository,
    log::*,
    std::{error::Error, fmt::Display, fs, path::PathBuf, str::FromStr, time::Instant},
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
    solana_root_path: PathBuf,
    release_channel: String,
}

impl BuildConfig {
    pub fn new(
        deploy_method: &str,
        do_build: bool,
        debug_build: bool,
        solana_root_path: &PathBuf,
        release_channel: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let deploy_method = deploy_method
            .parse::<DeployMethod>()
            .map_err(|_| "Failed to parse deploy_method".to_string())?;

        let build_path = match deploy_method {
            DeployMethod::Local => solana_root_path.join("farf/bin"),
            DeployMethod::Tar => solana_root_path.join("solana-release/bin"),
            DeployMethod::Skip => solana_root_path.join("farf/bin"),
        };

        Ok(BuildConfig {
            deploy_method,
            do_build,
            debug_build,
            _build_path: build_path,
            solana_root_path: solana_root_path.clone(),
            release_channel,
        })
    }

    pub async fn prepare(&self) -> Result<(), Box<dyn Error>> {
        match self.deploy_method {
            DeployMethod::Tar => {
                let file_name = "solana-release";
                match self.setup_tar_deploy(file_name).await {
                    Ok(tar_directory) => {
                        info!("Sucessfuly setup tar file");
                        cat_file(&tar_directory.join("version.yml")).unwrap();
                    }
                    Err(err) => {
                        error!("Failed to setup tar file! Did you set --release-channel? \
                        Or is there a solana-release.tar.bz2 file already in your solana/ root directory?");
                        return Err(err);
                    }
                }
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

    async fn setup_tar_deploy(&self, file_name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let tar_file = format!("{}{}", file_name, ".tar.bz2");
        info!("tar file: {}", tar_file);
        if !self.release_channel.is_empty() {
            match self.download_release_from_channel(file_name).await {
                Ok(_) => info!("Successfully downloaded tar release from channel"),
                Err(_) => error!("Failed to download tar release"),
            }
        } else {
            info!("No release channel set. Attempting to extract a local version of solana-release.tar.bz2...");
        }

        // Extract it and load the release version metadata
        let tarball_filename = self.solana_root_path.join(tar_file);
        let temp_release_dir = self.solana_root_path.join(file_name);
        extract_release_archive(&tarball_filename, &temp_release_dir, file_name).map_err(
            |err| {
                format!("Unable to extract {tarball_filename:?} into {temp_release_dir:?}: {err}")
            },
        )?;

        Ok(temp_release_dir)
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
            Err(err) => return Err(Box::new(err)),
        }

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

    async fn download_release_from_channel(&self, file_name: &str) -> Result<(), Box<dyn Error>> {
        info!("Downloading release from channel: {}", self.release_channel);
        let tar_file = format!("{}{}", file_name, ".tar.bz2");
        let file_path = self.solana_root_path.join(tar_file.as_str());
        // Remove file
        if let Err(err) = fs::remove_file(&file_path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(boxed_error!(format!(
                    "{}: {:?}",
                    "Error while removing file:", err
                )));
            }
        }

        let update_download_url = format!(
            "{}{}{}",
            "https://release.solana.com/",
            self.release_channel,
            "/solana-release-x86_64-unknown-linux-gnu.tar.bz2"
        );
        info!("update_download_url: {}", update_download_url);

        download_to_temp(
            update_download_url.as_str(),
            tar_file.as_str(),
            self.solana_root_path.clone(),
        )
        .await
        .map_err(|err| format!("Unable to download {update_download_url}: {err}"))?;

        Ok(())
    }
}
