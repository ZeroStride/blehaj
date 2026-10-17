use std::{io::IsTerminal, time::Duration};

use clap::{crate_name, crate_version};
use miette::IntoDiagnostic;
use tokio::{
    signal::unix::{SignalKind, signal},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use tracing_subscriber::{filter::EnvFilter, prelude::*};

use crate::network_manager::NetworkManager;

mod configuration;
mod error;
mod network_manager;

#[tokio::main]
async fn main() -> miette::Result<()> {
    miette::set_panic_hook();

    let configuration = configuration::Configuration::new();

    if configuration.json {
        miette::set_hook(Box::new(|_| Box::new(miette::JSONReportHandler)))?;
    } else if std::io::stdin().is_terminal() {
        miette::set_hook(Box::new(|_| {
            Box::new(miette::GraphicalReportHandler::default())
        }))?;
    } else {
        miette::set_hook(Box::new(|_| Box::new(miette::DebugReportHandler)))?;
    }

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
    let output_layer = if configuration.json {
        tracing_subscriber::fmt::layer().json().boxed()
    } else if configuration.systemd_launched
        && let Ok(journald_layer) = tracing_journald::layer()
    {
        journald_layer
            .with_custom_fields([(
                "UN_CONFIGURE_INTERFACE",
                configuration.network_interface.clone(),
            )])
            .boxed()
    } else {
        // Not sure if I want to test for is_terminal here or just assume
        // if std::io::stdin().is_terminal() ?
        tracing_subscriber::fmt::layer().compact().boxed()
    };

    tracing_subscriber::registry()
        .with(
            output_layer.with_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or(EnvFilter::new(format!("warn,{}=trace", crate_name!()))),
            ),
        )
        .init();

    info!(version = crate_version!(), ?configuration, "starting");

    //
    // Establish D-Bus connection, catch some errors I've run into
    //
    info!(interface = %configuration.network_interface, "connecting to D-Bus service org.freedesktop.NetworkManager");
    let network_manager = NetworkManager::new().await.map_err(|e| {
        // Shuld check e.to_string() == "failed to read from socket"
        if let network_manager::Error::ZbusSystemConnection { source: e } = e {
            return crate::error::Error::Actionable {
                        description: "failed to establish D-Bus connection".to_string(),
                        code: None,
                        advice: Some("If the systemd service unit is setting `DynamicUser=yes` it will cause an issue during the `NEGOTIATE_UNIX_FD` step.".to_string()),
                        source: Some(Box::new(e)),
                    }
        }
        e.into()
    })?;

    let network_device = network_manager
        .device(configuration.network_interface)
        .await
        .map_err(|e| {
            if let network_manager::Error::MethodError(
                network_manager::MethodError::UnknownDevice(ref name),
            ) = e
            {
                return crate::error::Error::Actionable {
                    description: format!("Interface '{}' not found.", name),
                    code: None,
                    advice: Some(
                        "use 'nmcli device status' to list the available devices".to_string(),
                    ),
                    source: Some(Box::new(e)),
                };
            }
            e.into()
        })?;

    let connectivity = network_manager.connectivity().await.into_diagnostic()?;
    info!(%connectivity, "current network connectivity");

    //
    // Initialize configuration mode(s)
    //
    // Do the stuff that can fail and would make the service unable to continue
    // to function, as configured, if it does fail.
    //

    // Todo - This should actually be configurable
    // let configuration_modes = vec![];
    info!("initializing bluetooth session");
    let session = bluer::Session::new().await.into_diagnostic()?;
    info!(?session, "getting bluetooth adapter");
    let adapter = match configuration.bluetooth_adapter {
        configuration::BluetoothAdapter::Default => {
            session.default_adapter().await.into_diagnostic()?
        }
        configuration::BluetoothAdapter::Named { name } => {
            session.adapter(&name).into_diagnostic()?
        }
        configuration::BluetoothAdapter::All => todo!(),
    };
    info!(?adapter, "power on");
    adapter.set_powered(true).await.into_diagnostic()?;

    //
    // Listen for signals
    //
    let cancelation_token = CancellationToken::new();
    {
        let cancelation_token = cancelation_token.clone();
        let mut sigterm = signal(SignalKind::terminate()).into_diagnostic()?;
        tokio::spawn(async move {
            let shutdown = |signal| {
                info!(%signal, "shutting down...");
                cancelation_token.cancel();
            };
            tokio::select! {
                _ = cancelation_token.cancelled() => {}
                _ = tokio::signal::ctrl_c() => {
                    shutdown("SIGINT");
                }
                _ = sigterm.recv() => {
                    shutdown("SIGTERM");
                }
            }
        });
    }

    //
    // So we either wait on a connectivity lost signal or we make it so the service
    // is only started, by a watchdog or something, when connectivity is lost.
    //

    //
    // Start up the designated configuration mode(s), and wait to receive a configuration
    //
    let mut configuration_tasks: JoinSet<Result<Option<()>, std::io::Error>> = JoinSet::new();

    loop {
        match configuration_tasks.join_next().await {
            None => break,
            Some(Err(e)) => {
                error!(%e, "error while joining configuration task");
            }
            Some(Ok(Err(e))) => {
                error!(%e, "error from configuration task");
            }
            Some(Ok(Ok(None))) => {
                debug!("task completed without returning configuration");
            }
            Some(Ok(Ok(Some(_configuration)))) => {
                let _checkpoint = network_manager
                    .build_checkpoint()
                    .for_device(&network_device)
                    .with_rollback_timeout(Duration::from_secs(15)) // Todo - Configurable delay
                    .destroy_all_on_create()
                    .build()
                    .await
                    .into_diagnostic()?;

                // Todo - Apply configuration

                // Todo - Validate with backend, then destroy checkpoint upon success
            }
        }
    }

    Ok(())
}
