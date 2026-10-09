use std::fmt;

use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothAdapter {
    All,
    First,
    Named { name: String },
}

impl fmt::Display for BluetoothAdapter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::First => write!(f, "first"),
            Self::Named { name } => fmt::Display::fmt(name, f),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Configuration {
    pub json: bool,
    pub bluetooth_adapter: BluetoothAdapter,
    pub network_interface: String,
    exit_after_configuration: bool,
}

impl Configuration {
    pub fn new() -> Self {
        let cli = ux::Cli::parse();

        Configuration {
            json: cli.json,
            bluetooth_adapter: {
                match cli.adapter {
                    None => BluetoothAdapter::First,
                    Some(adapter) => {
                        if let Some(name) = adapter.named {
                            BluetoothAdapter::Named { name }
                        } else if adapter.all.is_some() {
                            BluetoothAdapter::All
                        } else {
                            BluetoothAdapter::First
                        }
                    }
                }
            },
            network_interface: cli.interface,
            exit_after_configuration: cli.configuration,
        }
    }

    pub fn exit_after_configuration(&self) -> bool {
        self.exit_after_configuration
    }
}

mod ux {
    use clap::{Args, Parser};

    #[derive(Args, Clone, Debug)]
    #[group(required = false, multiple = false)]
    pub struct AdapterGroup {
        /// Use all compatible Bluetooth adapters.
        #[arg(hide_possible_values = true)]
        pub all: Option<bool>,

        /// Use the first compatible Bluetooth adapter. (Default)
        #[arg(hide_possible_values = true)]
        pub first: Option<bool>,

        /// Name of the Bluetooth adapter to use.
        #[arg(value_name = "<NAME>")]
        pub named: Option<String>,
    }

    #[derive(Parser)]
    #[command(version, about, long_about = None, propagate_version = true)]
    pub struct Cli {
        /// Network interface name to configure, e.g. "wlan0"
        pub interface: String,

        #[clap(flatten)]
        pub adapter: Option<AdapterGroup>,

        /// Write configuration to stdout and exit.
        #[arg(long)]
        pub configuration: bool,

        /// Output any configuration, or dry-run output as JSON.
        #[arg(long)]
        pub json: bool,
    }
}
