use std::{collections::HashMap, io::IsTerminal, time::Duration};

use clap::{crate_name, crate_version};
use miette::IntoDiagnostic;
use tokio::{
    signal::unix::{SignalKind, signal},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use tracing_subscriber::{filter::EnvFilter, prelude::*};
use zbus::zvariant;

use crate::{
    ble_peripheral_configuration_source::BlePeripheralConfigurationSource,
    network_manager::NetworkManager, zvariant::Signature,
};

mod ble_peripheral_configuration_source;
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
        .device(&configuration.network_interface)
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
    let configuration_source =
        BlePeripheralConfigurationSource::new_from_configuration(&configuration)
            .await
            .into_diagnostic()?;

    //
    // Listen for signals
    //
    let termination_token = CancellationToken::new();
    {
        let termination_token = termination_token.clone();
        let mut sigterm = signal(SignalKind::terminate()).into_diagnostic()?;
        tokio::spawn(async move {
            let shutdown = |signal| {
                info!(%signal, "shutting down...");
                termination_token.cancel();
            };
            tokio::select! {
                _ = termination_token.cancelled() => {}
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
    // Main service/wait loop
    //
    #[allow(clippy::never_loop)]
    loop {
        //
        // So we either wait on a connectivity lost signal or we make it so the service
        // is only started, by a watchdog or something, when connectivity is lost.
        //
        if termination_token.is_cancelled() {
            break;
        }

        //
        // Start up the designated configuration mode(s), and wait to receive a configuration
        //
        let cancel_configuration_token = termination_token.child_token();
        let mut configuration_tasks: JoinSet<
            Result<Option<Vec<u8>>, ble_peripheral_rust::error::Error>,
        > = JoinSet::new();

        configuration_tasks.spawn(configuration_source.proc(cancel_configuration_token.clone()));

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
                Some(Ok(Ok(Some(configuration_bytes)))) => {
                    info!("deserializing configuration json");
                    match serde_json::from_slice::<HashMap<&str, HashMap<&str, zvariant::Value>>>(
                        &configuration_bytes,
                    ) {
                        Ok(connection_settings) => {
                            info!(
                                ?connection_settings,
                                "connection received from configuration source"
                            );

                            // Unpack into connection settings, and fix the data type of the SSID.
                            let connection_settings =
                                if *connection_settings["802-11-wireless"]["ssid"].value_signature()
                                    == Signature::Str
                                {
                                    // The string gets double-quotes around it when it deserializes from
                                    // the  zvariant Value type.
                                    let ssid_string =
                                        connection_settings["802-11-wireless"]["ssid"]
                                            .to_string()
                                            .trim_matches('\"')
                                            .to_string();

                                    let mut connection_settings = connection_settings;
                                    if let Some(wireless_802_11) =
                                        connection_settings.get_mut("802-11-wireless")
                                        && let Some(ssid) = wireless_802_11.get_mut("ssid")
                                    {
                                        *ssid = ssid_string.into_bytes().into();
                                    }
                                    connection_settings
                                } else {
                                    connection_settings
                                };

                            let _checkpoint = network_manager
                                .build_checkpoint()
                                .for_device(&network_device)
                                .with_rollback_timeout(Duration::from_secs(15)) // Todo - Configurable delay
                                .destroy_all_on_create()
                                .build()
                                .await
                                .into_diagnostic()?;

                            // Todo - Apply configuration
                            // network_manager
                            //     .add_and_activate_connection(
                            //         connection_settings,
                            //         &network_interface_path,
                            //         &"/".try_into().into_diagnostic()?,
                            //     )
                            //     .await
                            //     .into_diagnostic()?;

                            // Todo - Validate with backend, then destroy checkpoint upon success
                        }
                        Err(e) => {
                            error!(%e, "deserialization error");
                        }
                    }
                }
            }
        }

        // Hack - Just break here for development.
        break;
    }

    Ok(())
}
