//! OxideDAW - Main entry point

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting OxideDAW v0.1.0...");

    let project = oxide_core::Project::default();
    tracing::info!("Loaded project: '{}' @ {} BPM", project.name, project.bpm);
}
