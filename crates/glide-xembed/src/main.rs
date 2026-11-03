use std::process::ExitCode;

fn main() -> ExitCode {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).init();
    match glide_xembed::serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("glide-xembed: {err:#}");
            ExitCode::FAILURE
        }
    }
}
