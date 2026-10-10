use clap::crate_version;
use miette::IntoDiagnostic;
use tracing::info;
use tracing_subscriber::prelude::*;
use zbus::Connection;

mod configuration;
mod error;
mod network_manager;

#[tokio::main]
async fn main() -> miette::Result<()> {
    miette::set_panic_hook();

    let configuration = configuration::Configuration::new();

    if configuration.json {
        miette::set_hook(Box::new(|_| Box::new(miette::JSONReportHandler)))?;
    } else {
        miette::set_hook(Box::new(|_| {
            Box::new(miette::GraphicalReportHandler::default())
        }))?;
    }
    // The GraphicalReportHandler seems to actually look fine in journalctl
    // } else if std::io::stdin().is_terminal() {
    //     miette::set_hook(Box::new(|_| {
    //         Box::new(miette::GraphicalReportHandler::default())
    //     }))?;
    // } else {
    //     miette::set_hook(Box::new(|_| Box::new(miette::DebugReportHandler)))?;
    // }

    #[allow(clippy::print_stdout)]
    if configuration.exit_after_configuration {
        if configuration.json {
            serde_json::to_writer(std::io::stdout(), &configuration).into_diagnostic()?;
        } else {
            println!("{:?}", configuration);
        }
        return Ok(());
    }

    // Tracing
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

    info!(version = crate_version!(), ?configuration, "starting");

    // Get interface to configure, will error if not found
    info!(interface = %configuration.network_interface, "connecting to D-Bus service org.freedesktop.NetworkManager");
    // DynamicUser
    let connection = Connection::system()
        .await
        .map_err(|e| {
            if let zbus::Error::InputOutput(ref e) = e
                && e.to_string() == "failed to read from socket" {
                    let e = e.clone();
                    // Todo - Could try reading the .service file and see if this is the case?
                    return crate::error::Error::Actionable {
                        description: "failed to establish D-Bus connection".to_string(),
                        code: None,
                        advice: Some("If the systemd service unit is setting `DynamicUser=yes` it will cause an issue during the `NEGOTIATE_UNIX_FD` step.".to_string()),
                        source: Some(Box::new(e)),
                    }
                }
            e.into()
        })?;

    let network_manager = network_manager::NetworkManagerProxy::new(&connection)
        .await
        .into_diagnostic()?;
    let network_interface_path = network_manager
        .get_device_by_ip_iface(&configuration.network_interface)
        .await
        .map_err(|e| {
            if let zbus::Error::MethodError(ref error_name, ..) = e
                && *error_name == "org.freedesktop.NetworkManager.UnknownDevice"
            {
                return crate::error::Error::Actionable {
                    description: format!(
                        "Interface '{}' not found.",
                        configuration.network_interface
                    ),
                    code: None,
                    advice: Some(
                        "use 'nmcli device status' to list the available devices".to_string(),
                    ),
                    source: Some(Box::new(e.clone())),
                };
            }
            e.into()
        })?;

    Ok(())
}
