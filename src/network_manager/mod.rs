mod generated;

use std::{collections::HashMap, time::Duration};

use generated::NetworkManagerProxy;
use thiserror::Error;
use tracing::{instrument, trace};
use zbus::{Connection, ObjectPath, OwnedObjectPath};

/// NetworkManager errors.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{source}")]
    Zbus {
        #[source]
        source: zbus::Error,
    },

    /// Error while establishing a D-Bus System Bus Connection.
    #[error("failed to establish D-Bus system bus connection")]
    ZbusSystemConnection {
        #[source]
        source: zbus::Error,
    },

    #[error(transparent)]
    MethodError(#[from] MethodError),

    #[error("argument error {description}")]
    Argument {
        description: String,

        #[source]
        source: Option<Box<dyn std::error::Error>>,
    },
}

unsafe impl Send for Error {}
unsafe impl Sync for Error {}

impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        if let zbus::Error::MethodError(ref _error_name, ..) = e {
            Error::MethodError(MethodError::from(e))
        } else {
            Error::Zbus { source: e }
        }
    }
}

/// NetworkManager method invocation errors.
#[derive(Debug, Error)]
pub enum MethodError {
    /// Unknown Device
    #[error("Unknown Device '{0}'")]
    UnknownDevice(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Generic error
    #[error("{name}")]
    Other {
        name: String,
        detail: Option<String>,
        reply: zbus::Message,
    },
}

unsafe impl Send for MethodError {}
unsafe impl Sync for MethodError {}

impl From<zbus::Error> for MethodError {
    fn from(e: zbus::Error) -> Self {
        if let zbus::Error::MethodError(ref name, ref detail, ref reply) = e {
            match name.as_str() {
                "org.freedesktop.NetworkManager.PermissionDenied" => {
                    return MethodError::PermissionDenied(
                        detail
                            .clone()
                            .unwrap_or("(violation not specified)".to_string()),
                    );
                }
                "org.freedesktop.NetworkManager.UnknownDevice" => {
                    unimplemented!(
                        "'org.freedesktop.NetworkManager.UnknownDevice' unimplemented, usually caught & created specifically"
                    );
                }
                &_ => {
                    trace!(?detail, ?reply, "zbus::Error '{}'", name);
                    return MethodError::Other {
                        name: name.to_string(),
                        detail: detail.clone(),
                        reply: reply.clone(),
                    };
                }
            }
        }
        unreachable!("not a zbus::Error::MethodError {:?}", e);
    }
}

/// The NetworkManage rconnectivity state.
pub enum ConnectivityState {
    /// Networking state is unknown.
    /// This indicates a daemon error that makes it unable to reasonably assess the state.
    /// In such event the applications are expected to assume Internet connectivity might
    /// be present and not disable controls that require network access.
    /// The graphical shells may hide the network accessibility indicator altogether
    /// since no meaningful status indication can be provided.
    Unknown = 0,

    /// NetworkManager is disabled, either because the user requested to disable networking
    /// or because the system is suspended or resuming from suspend.
    Disabled = 10,

    /// There is no active network connection.
    /// The graphical shell should indicate no network connectivity and
    /// the applications should not attempt to access the network.
    Disconnected = 20,

    /// Network connections are being cleaned up.
    /// The applications should tear down their network sessions.
    Disconnecting = 30,

    /// A network connection is being started.
    /// The graphical shell should indicate the network is being connected while the applications should
    /// still make no attempts to connect the network.
    Connecting = 40,

    /// There is only local IPv4 and/or IPv6 connectivity, but no default route to access the Internet.
    /// The graphical shell should indicate no network connectivity.
    ConnectedLocal = 50,

    /// There is only site-wide IPv4 and/or IPv6 connectivity. This means a default route is available,
    /// but the Internet connectivity check (see "Connectivity" property) did not succeed.
    /// The graphical shell should indicate limited network connectivity.
    ConnectedSite = 60,

    /// There is global IPv4 and/or IPv6 Internet connectivity.
    // This means the Internet connectivity check succeeded,
    /// the graphical shell should indicate full network connectivity.
    ConnectedGlobal = 70,
}

impl TryFrom<u32> for ConnectivityState {
    type Error = Error;
    fn try_from(x: u32) -> Result<Self, Self::Error> {
        match x {
            0 => Ok(Self::Unknown),
            10 => Ok(Self::Disabled),
            20 => Ok(Self::Disconnected),
            30 => Ok(Self::Disconnecting),
            40 => Ok(Self::Connecting),
            50 => Ok(Self::ConnectedLocal),
            60 => Ok(Self::ConnectedSite),
            70 => Ok(Self::ConnectedGlobal),
            _ => Err(Error::Argument {
                description: format!("unknown state value {x}").to_string(),
                source: None,
            }),
        }
    }
}

impl std::fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Disabled => write!(f, "Disabled"),
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Disconnecting => write!(f, "Disconnecting"),
            Self::Connecting => write!(f, "Connecting"),
            Self::ConnectedLocal => write!(f, "ConnectedLocal"),
            Self::ConnectedSite => write!(f, "ConnectedSite"),
            Self::ConnectedGlobal => write!(f, "ConnectedGlobal"),
        }
    }
}

pub struct NetworkManager<'proxy> {
    _system_bus_connection: zbus::Connection,
    proxy: generated::NetworkManagerProxy<'proxy>,
}

impl NetworkManager<'_> {
    #[instrument]
    pub async fn new() -> Result<Self, Error> {
        trace!("establishing connection to system bus");
        let system_bus_connection = Connection::system()
            .await
            .map_err(|e| Error::ZbusSystemConnection { source: e })?;
        trace!(system_bus_connection = ?system_bus_connection.unique_name(), "creating proxy");
        let proxy = NetworkManagerProxy::new(&system_bus_connection).await?;
        Ok(NetworkManager {
            _system_bus_connection: system_bus_connection,
            proxy,
        })
    }

    #[instrument(skip(self))]
    pub async fn device(&self, name: &String) -> Result<Device<'_>, Error> {
        trace!("getting device");
        Ok(Device {
            path: self.proxy.get_device_by_ip_iface(name).await.map_err(|e| {
                if let zbus::Error::MethodError(ref error_name, ..) = e
                    && *error_name == "org.freedesktop.NetworkManager.UnknownDevice"
                {
                    return MethodError::UnknownDevice(name.to_string());
                }
                e.into()
            })?,
            manager: self,
        })
    }

    #[instrument(skip(self))]
    pub fn build_checkpoint(&self) -> CheckpointBuilder<'_> {
        CheckpointBuilder::new(self)
    }

    #[instrument(skip(self))]
    pub async fn connectivity(&self) -> Result<ConnectivityState, Error> {
        trace!("getting state");
        self.proxy
            .state()
            .await
            .map_err(Error::from)
            .map(ConnectivityState::try_from)?
    }
}

pub struct Device<'manager> {
    path: OwnedObjectPath,
    manager: &'manager NetworkManager<'manager>,
}

impl Device<'_> {
    #[instrument(skip(self, connection_settings))]
    pub async fn add_and_activate_connection(
        &self,
        connection_settings: HashMap<&str, HashMap<&str, zbus::Variant<'_>>>,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), Error> {
        let uuid: String = connection_settings["connection"]["uuid"]
            .to_string()
            .trim_matches('\"')
            .to_string();
        trace!(connection = ?connection_settings["connection"], "creating connection");
        let path_tuple = self
            .manager
            .proxy
            .add_and_activate_connection(
                connection_settings,
                &self.path,
                &"/".try_into().map_err(Error::from)?,
            )
            .await
            .map_err(Error::from)?;
        trace!(
            uuid,
            connection = ?path_tuple.0,
            active_connection = ?path_tuple.1,
            "connection added and active"
        );
        Ok(path_tuple)
    }
}

pub struct Checkpoint<'manager> {
    network_manager: &'manager NetworkManager<'manager>,
    path: OwnedObjectPath,
}

impl Checkpoint<'_> {
    #[instrument(skip(self))]
    pub async fn rollback(self) -> Result<(), Error> {
        // Todo - Implement NMRollbackResult
        trace!(%self.path, "rolling back checkpoint");
        self.network_manager
            .proxy
            .checkpoint_rollback(&self.path)
            .await
            .map(|_| Ok(()))
            .map_err(Error::from)?
    }

    #[instrument(skip(self))]
    pub async fn commit(self) -> Result<(), Error> {
        trace!(%self.path, "destroying checkpoint");
        self.network_manager
            .proxy
            .checkpoint_destroy(&self.path)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    pub async fn extend(self, new_timeout: Duration) -> Result<(), Error> {
        trace!(%self.path, "adjusting rollback timeout for checkpoint");
        let new_timeout_secs: u32 =
            new_timeout
                .as_secs()
                .try_into()
                .map_err(|e| Error::Argument {
                    description: "Duration provided to 'extend' exceeded u32::MAX seconds"
                        .to_string(),
                    source: Some(Box::new(e)),
                })?;

        self.network_manager
            .proxy
            .checkpoint_adjust_rollback_timeout(&self.path, new_timeout_secs)
            .await
            .map_err(Error::from)
    }
}

impl std::fmt::Debug for Checkpoint<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Checkpoint")
            .field("path", &self.path)
            .field("flags", &"todo")
            .finish()
    }
}

pub struct CheckpointBuilder<'manager> {
    network_manager: &'manager NetworkManager<'manager>,
    devices: Option<Vec<&'manager ObjectPath<'manager>>>,
    rollback_timeout: Option<u64>,
    flags: u32,
}

impl std::fmt::Debug for CheckpointBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.debug_struct("CheckpointBuilder")
            .field("devices", &self.devices)
            .field("rollback_timeout", &self.rollback_timeout)
            .field("flags", &self.flags)
            .finish()
    }
}

impl<'manager> CheckpointBuilder<'manager> {
    const NM_CHECKPOINT_CREATE_FLAG_DESTROY_ALL: u32 = 0x01;
    const NM_CHECKPOINT_CREATE_FLAG_DELETE_NEW_CONNECTIONS: u32 = 0x02;

    fn new(network_manager: &'manager NetworkManager<'_>) -> Self {
        CheckpointBuilder {
            network_manager,
            devices: None,
            rollback_timeout: None,
            flags: 0,
        }
    }

    /// Builder for a Checkpoint.
    #[instrument(skip(self))]
    pub async fn build(self) -> Result<Checkpoint<'manager>, Error> {
        trace!(builder = ?self, "creating checkpoint");
        let path = self.network_manager.proxy.checkpoint_create(
            &self.devices.ok_or(Error::Argument {
                description: "must add devices to the Checkpoint using 'for_device', 'for_devices', or specify 'for_all_devices'".to_string(),
                source: None,
            })?,
            self.rollback_timeout
                .ok_or(Error::Argument {
                    description: "'with_rollback_timeout' not specified".to_string(),
                    source: None,
                })?.try_into()
                .map_err(|e| Error::Argument {
                    description:
                        "Duration provided to 'with_rollback_timeout' exceeded u32::MAX seconds"
                            .to_string(),
                    source: Some(Box::new(e)),
                })?,
            self.flags,
        ).await?;
        Ok(Checkpoint {
            network_manager: self.network_manager,
            path,
        })
    }

    /// All devices included in this checkpoint.
    #[must_use]
    pub fn for_all_devices(mut self) -> Self {
        self.devices = Some(vec![]);
        self
    }

    /// Set (or append) devices for the checkpoint.
    #[must_use]
    pub fn for_device(self, device: &'manager Device) -> Self {
        self.for_devices(vec![device])
    }

    /// Set (or append) devices for the checkpoint.
    #[must_use]
    pub fn for_devices(mut self, devices: std::vec::Vec<&'manager Device>) -> Self {
        let new_devices = devices.into_iter().map(|x| &*x.path).collect();
        self.devices = match self.devices {
            None => Some(new_devices),
            Some(devices) => Some([devices, new_devices].concat()),
        };
        self
    }

    /// Rollback after the provided duration.
    #[must_use]
    pub fn with_rollback_timeout(mut self, duration: Duration) -> Self {
        self.rollback_timeout = Some(duration.as_secs());
        self
    }

    /// Destroy all other checkpoints when creating this checkpoint.
    #[must_use]
    pub fn destroy_all_on_create(mut self) -> Self {
        self.flags |= Self::NM_CHECKPOINT_CREATE_FLAG_DESTROY_ALL;
        self
    }

    /// When this checkpoint is rolled back, delete any new connections that were
    /// added after this checkpoint was created.
    #[must_use]
    pub fn delete_new_connections_on_rollback(mut self) -> Self {
        self.flags |= Self::NM_CHECKPOINT_CREATE_FLAG_DELETE_NEW_CONNECTIONS;
        self
    }
}
