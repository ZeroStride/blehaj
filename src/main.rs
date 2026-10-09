use tracing::info;
use tracing_subscriber::prelude::*;

mod configuration;

fn main() -> miette::Result<()> {
    miette::set_panic_hook();

    let configuration = configuration::Configuration::new();

    {
        let stdout_layer = if configuration.json {
            tracing_subscriber::fmt::layer().json().boxed()
        } else {
            tracing_subscriber::fmt::layer().compact().boxed()
        };

        let journald_layer = tracing_journald::layer()
            .map(|layer| {
                layer.with_custom_fields([(
                    "UN_CONFIGURE_INTERFACE",
                    configuration.network_interface.clone(),
                )])
            })
            .ok();

        tracing_subscriber::registry()
            .with(journald_layer)
            .with(stdout_layer)
            .init()
    }

    info!("{:?}", configuration);
    Ok(())
}
