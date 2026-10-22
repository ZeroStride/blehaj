use std::fmt;

use clap::Parser;
use derive_more::Debug;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothAdapter {
    All,
    Default,
    Named { name: String },
}

impl fmt::Display for BluetoothAdapter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Default => write!(f, "default"),
            Self::Named { name } => fmt::Display::fmt(name, f),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Configuration {
    pub json: bool,
    // pub bluetooth_adapter: BluetoothAdapter,
    pub bluetooth_service_uuid: Uuid,
    pub network_interface: String,
    pub exit_after_configuration: bool,
    pub systemd_launched: bool,
    pub no_journald: bool,
    pub qr_code_url_prefix: Option<String>,
    #[cfg(feature = "digest")]
    #[serde(skip)]
    #[debug(skip)]
    pub signing_key: Option<ed25519_dalek::SigningKey>,
}

impl Configuration {
    #[allow(clippy::print_stderr)]
    pub fn new() -> Self {
        let cli = ux::Cli::parse();

        Configuration {
            json: cli.json,
            // bluetooth_adapter: {
            //     match cli.adapter {
            //         None => BluetoothAdapter::Default,
            //         Some(adapter) => {
            //             if let Some(name) = adapter.named {
            //                 BluetoothAdapter::Named { name }
            //             } else if adapter.all.is_some() {
            //                 BluetoothAdapter::All
            //             } else {
            //                 BluetoothAdapter::Default
            //             }
            //         }
            //     }
            // },
            bluetooth_service_uuid: cli.service_uuid.unwrap_or(Uuid::new_v4()),
            network_interface: cli.interface,
            exit_after_configuration: cli.configuration,
            systemd_launched: std::os::unix::process::parent_id() == 1,
            no_journald: cli.no_journald,
            qr_code_url_prefix: cli.qr_code_url_prefix.map(String::from),
            #[cfg(feature = "digest")]
            signing_key: cli.signing_key,
        }
    }
}

mod ux {
    use clap::Parser;
    use url::Url;
    use uuid::Uuid;

    // #[derive(Args, Clone, Debug)]
    // #[group(required = false, multiple = false)]
    // pub struct AdapterGroup {
    //     /// Use all compatible Bluetooth adapters.
    //     #[arg(hide_possible_values = true)]
    //     pub all: Option<bool>,

    //     /// Use the default Bluetooth adapter. (Default)
    //     #[arg(hide_possible_values = true)]
    //     pub default: Option<bool>,

    //     /// Name of the Bluetooth adapter to use.
    //     #[arg(value_name = "<NAME>")]
    //     pub named: Option<String>,
    // }

    #[derive(Parser)]
    #[command(version, about, long_about = None, propagate_version = true)]
    pub struct Cli {
        /// Network interface name to configure, e.g. "wlan0"
        pub interface: String,

        // #[clap(flatten)]
        // pub adapter: Option<AdapterGroup>,
        /// Write configuration to stdout and exit.
        #[arg(long)]
        pub configuration: bool,

        /// Output any configuration, or dry-run output as JSON.
        #[arg(long)]
        pub json: bool,

        /// Static Bluetooth Service UUID
        #[arg(long)]
        pub service_uuid: Option<Uuid>,

        /// Do not log to journald, even if launched via systemd (or systemd-run)
        #[arg(long)]
        pub no_journald: bool,

        /// Url prefix for QR Code Generation, e.g. "http://localhost:3000/?uuid="
        #[cfg(feature = "qrcode")]
        #[arg(long)]
        pub qr_code_url_prefix: Option<Url>,

        /// Private key used to sign requests for configuration. Note: Only ed25519 keys are supported, currently.
        #[cfg(feature = "digest")]
        #[arg(long, value_parser = crate::digest::parse_signing_key)]
        pub signing_key: Option<ed25519_dalek::SigningKey>,
    }
}
