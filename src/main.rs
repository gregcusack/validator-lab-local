use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches},
    log::*,
    validator_lab::{
        kubernetes::Kubernetes,
        release::{BuildConfig, DeployMethod},
        SolanaRoot,
    },
};

fn parse_matches() -> ArgMatches<'static> {
    App::new(crate_name!())
        .about(crate_description!())
        .arg(
            Arg::with_name("cluster_namespace")
                .long("namespace")
                .short("n")
                .takes_value(true)
                .default_value("default")
                .help("namespace to deploy test cluster"),
        )
        .arg(
            Arg::with_name("deploy_method")
                .long("deploy-method")
                .takes_value(true)
                .possible_values(&["local", "tar", "skip"])
                .default_value("local")
                .help("Deploy method. tar, local, skip. [default: local]"),
        )
        .arg(
            Arg::with_name("local-path")
                .long("local-path")
                .takes_value(true)
                .required_if("deploy-method", "local")
                .conflicts_with_all(&["tar", "skip"])
                .help("Path to local agave repo. Required for 'local' deploy method."),
        )
        .arg(
            Arg::with_name("do_build")
                .long("do-build")
                .help("Enable building for building from local repo"),
        )
        .arg(
            Arg::with_name("debug_build")
                .long("debug-build")
                .help("Enable debug build"),
        )
        .arg(
            Arg::with_name("release_channel")
                .long("release-channel")
                .takes_value(true)
                .required_if("deploy_method", "tar") // Require if deploy_method is "tar"
                .help("release version. e.g. v1.17.2. Required if '--deploy-method tar'"),
        )
        .get_matches()
}

#[derive(Clone, Debug)]
pub struct EnvironmentConfig<'a> {
    pub namespace: &'a str,
}

#[tokio::main]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();
    let matches = parse_matches();
    let environment_config = EnvironmentConfig {
        namespace: matches.value_of("cluster_namespace").unwrap_or_default(),
    };

    let deploy_method = matches.value_of("deploy_method").unwrap();
    let local_path = matches.value_of("local-path");
    match deploy_method {
        method if method == DeployMethod::Local.to_string() => {
            if local_path.is_none() {
                panic!("Error: --local-path is required for 'local' deploy-method.");
            }
        }
        _ => {
            if local_path.is_some() {
                warn!("WARN: --local-path <path> will be ignored");
            }
        }
    }

    let solana_root = match matches.value_of("local-path") {
        Some(path) => SolanaRoot::new_from_path(path.into()),
        None => SolanaRoot::default(),
    };

    let kub_controller = Kubernetes::new(environment_config.namespace).await;
    match kub_controller.namespace_exists().await {
        Ok(true) => (),
        Ok(false) => {
            error!(
                "Namespace: '{}' doesn't exist. Exiting...",
                environment_config.namespace
            );
            return;
        }
        Err(err) => {
            error!("Error: {}", err);
            return;
        }
    }

    let build_config = BuildConfig::new(
        deploy_method,
        matches.is_present("do_build"),
        matches.is_present("debug_build"),
        &solana_root.get_root_path(),
        matches
            .value_of("release_channel")
            .unwrap_or_default()
            .to_string(),
    )
    .unwrap_or_else(|err| {
        panic!("Error creating BuildConfig: {}", err);
    });

    match build_config.prepare().await {
        Ok(_) => info!("Validator setup prepared successfully"),
        Err(err) => {
            error!("Error: {}", err);
            return;
        }
    }
}
