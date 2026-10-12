mod generated;

use generated::NetworkManagerProxy;
use thiserror::Error;
use tracing::{instrument, trace};
use zbus::{Connection, zvariant::OwnedObjectPath};

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
}

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

    #[error("{name}")]
    Other {
        name: String,
        detail: Option<String>,
        reply: zbus::Message,
    },
}

impl From<zbus::Error> for MethodError {
    fn from(e: zbus::Error) -> Self {
        if let zbus::Error::MethodError(ref name, ref detail, ref reply) = e {
            return MethodError::Other {
                name: name.to_string(),
                detail: detail.clone(),
                reply: reply.clone(),
            };
        }
        unreachable!("not a zbus::Error::MethodError {:?}", e);
    }
}

pub struct NetworkManager<'proxy> {
    system_bus_connection: zbus::Connection,
    proxy: generated::NetworkManagerProxy<'proxy>,
}

impl NetworkManager<'_> {
    #[instrument]
    pub async fn new() -> Result<Self, Error> {
        trace!("establishing connection to system bus");
        let system_bus_connection = Connection::system()
            .await
            .map_err(|e| Error::ZbusSystemConnection { source: e })?;
        trace!(?system_bus_connection, "creating proxy");
        let proxy = NetworkManagerProxy::new(&system_bus_connection).await?;
        Ok(NetworkManager {
            system_bus_connection,
            proxy,
        })
    }

    #[instrument(skip(self))]
    pub async fn device(&self, name: String) -> Result<Device, Error> {
        Ok(Device::new(
            self.proxy
                .get_device_by_ip_iface(&name)
                .await
                .map_err(|e| {
                    if let zbus::Error::MethodError(ref error_name, ..) = e {
                        if *error_name == "org.freedesktop.NetworkManager.UnknownDevice" {
                            return MethodError::UnknownDevice(name);
                        }
                        return MethodError::from(e);
                    }
                    e.into()
                })?,
        ))
    }
}

pub struct Device {
    path: OwnedObjectPath,
}

impl Device {
    #[instrument]
    fn new(path: OwnedObjectPath) -> Self {
        Device { path }
    }
}

pub struct Checkpoint {}
