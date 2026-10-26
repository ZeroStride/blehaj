use std::{io::prelude::*, sync::Arc};

use ble_peripheral_rust::{
    Peripheral, PeripheralImpl,
    gatt::{
        characteristic::Characteristic,
        peripheral_event::{
            PeripheralEvent, PeripheralRequest, ReadRequestResponse, RequestResponse,
            WriteRequestResponse,
        },
        properties::{AttributePermission, CharacteristicProperty},
        service::Service,
    },
};
#[cfg(feature = "digest")]
use ed25519_dalek::Signer;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, mpsc::Receiver, oneshot::Sender};
use tokio_util::{future::FutureExt, sync::CancellationToken};
use tracing::{debug, error, info, instrument, warn};
use uuid::{Uuid, uuid};

use crate::configuration::{self};

const CONFIGURATION_CHARACTERISTIC_UUID: Uuid = uuid!("4b3bd2c3-2560-4353-8d37-05ca8333d01e");

#[derive(Debug, Deserialize, Serialize)]
enum Request {
    NetworkManagerConfiguration { interface: String },
}

#[derive(Debug, Deserialize, Serialize)]
struct Envelope {
    pub identifier: String,
    pub cypher: String,
    pub data: Request,
}

#[derive(Deserialize, Serialize)]
struct SignedPayload {
    pub signature: String,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl TryInto<Vec<u8>> for SignedPayload {
    type Error = rmp_serde::encode::Error;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        rmp_serde::to_vec_named(&self)
    }
}

impl TryInto<Vec<u8>> for Envelope {
    type Error = rmp_serde::encode::Error;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        rmp_serde::to_vec_named(&self)
    }
}

pub struct BlePeripheralConfigurationSource {
    // session: bluer::Session,
    // adapter: bluer::Adapter,
    peripheral: Peripheral,
    event_rx: Receiver<PeripheralEvent>,
    service: Service,
    configuration: Arc<configuration::Configuration>,
}

impl BlePeripheralConfigurationSource {
    #[instrument]
    pub async fn new_from_configuration(
        configuration: Arc<configuration::Configuration>,
    ) -> Result<Self, ble_peripheral_rust::error::Error> {
        //bluer::Error> {
        // info!("initializing bluetooth session");
        // let session = bluer::Session::new().await?;
        // info!(?session, "getting bluetooth adapter");
        // let adapter = match configuration.bluetooth_adapter {
        //     configuration::BluetoothAdapter::Default => session.default_adapter().await?,
        //     configuration::BluetoothAdapter::Named { ref name } => session.adapter(name)?,
        //     configuration::BluetoothAdapter::All => todo!(),
        // };
        // info!(?adapter, "power on");
        // adapter.set_powered(true).await?;

        // Ok(BlePeripheralConfigurationSource { session, adapter })

        info!("initializing bluetooth peripheral");
        let (event_tx, event_rx) = mpsc::channel::<PeripheralEvent>(256);
        let peripheral = Peripheral::new(event_tx).await?;
        Ok(BlePeripheralConfigurationSource {
            peripheral,
            event_rx,
            service: Service {
                uuid: configuration.bluetooth_service_uuid,
                primary: true,
                characteristics: vec![Characteristic {
                    uuid: CONFIGURATION_CHARACTERISTIC_UUID,
                    properties: vec![
                        CharacteristicProperty::Read,
                        CharacteristicProperty::Write,
                        CharacteristicProperty::Notify,
                    ],
                    permissions: vec![
                        AttributePermission::Readable,
                        AttributePermission::Writeable,
                    ],
                    value: None,
                    ..Default::default()
                }],
            },
            configuration,
        })
    }

    #[instrument(skip(self))]
    async fn state_update(&self, is_powered: bool) {
        debug!("state update");
    }

    #[instrument(skip(self, responder))]
    async fn read_request(
        &self,
        request: PeripheralRequest,
        responder: Sender<ReadRequestResponse>,
    ) {
        let configuration_request = Envelope {
            identifier: Uuid::new_v4().to_string(),
            cypher: "none".to_owned(),
            data: Request::NetworkManagerConfiguration {
                interface: self.configuration.network_interface.clone(),
            },
        };
        debug!(response = ?configuration_request, "responding to read_request");

        let value = cfg_select! {
            feature = "digest" => {
                if let Some(ref signing_key) = self.configuration.signing_key {
                    let bytes_to_sign: Vec<u8> = configuration_request
                        .try_into()
                        .expect("serialization failed");
                    match signing_key.try_sign(&bytes_to_sign) {
                        Ok(sig) => {
                            let sig = hex::encode(sig.to_bytes());
                            debug!(sig, "signature");
                            SignedPayload {
                                signature: sig,
                                payload: bytes_to_sign,
                            }
                        }
                        Err(e) => {
                            error!(%e, "error while signing payload");
                            SignedPayload {
                                signature: String::new(),
                                payload: bytes_to_sign,
                            }
                        }
                    }
                } else {
                    warn!("no signing key provided, responses are unsigned");
                    SignedPayload {
                        signature: String::new(),
                        payload: configuration_request
                            .try_into()
                            .expect("serialization failed"),
                    }
                }
            }
            _ => BleEnvelope {
                signature: String::new(),
                request: configuration_request
                    .try_into()
                    .expect("serialization failed"),
            },
        };

        if responder
            .send(ReadRequestResponse {
                value: value.try_into().expect("serialization failed"),
                response: RequestResponse::Success,
            })
            .is_err()
        {
            error!("error sending read_request response, receiver dropped");
        }
    }

    #[instrument(skip(self, value, responder))]
    async fn write_request(
        &self,
        request: PeripheralRequest,
        value: Vec<u8>,
        responder: Sender<WriteRequestResponse>,
    ) -> Option<Vec<u8>> {
        if responder
            .send(WriteRequestResponse {
                response: RequestResponse::Success,
            })
            .is_err()
        {
            error!("error sending write_request response, receiver dropped");
        }
        debug!("received data from write_request");
        let mut decoder = GzDecoder::new(&value[..]);
        let mut vec = Vec::new();
        match decoder.read_to_end(&mut vec) {
            Ok(inflated_size) => {
                debug!(size = value.len(), inflated_size, "inflated bytes");
                Some(vec)
            }
            Err(e) => {
                error!(%e, "error inflating data");
                None
            }
        }
    }

    #[instrument(skip(self))]
    async fn subscribed(&self, request: PeripheralRequest) {
        debug!("subscribed");
    }

    #[instrument(skip(self))]
    async fn unsubscribed(&self, request: PeripheralRequest) {
        debug!("unsubscribed");
    }

    #[instrument(skip(self, cancellation_token))]
    pub async fn proc(
        mut self,
        cancellation_token: CancellationToken,
    ) -> Result<Option<Vec<u8>>, ble_peripheral_rust::error::Error> {
        debug!("powering on peripheral");
        while !self.peripheral.is_powered().await? {}

        debug!(?self.service, "adding service");
        self.peripheral.add_service(&self.service).await?;

        info!(%self.service.uuid, "advertising service");
        self.peripheral
            .start_advertising("blehaj-todo-add-setting", &[self.service.uuid])
            .await?;

        #[cfg(feature = "qrcode")]
        if let Some(ref qr_code_url_prefix) = self.configuration.qr_code_url_prefix {
            use qrcode_generator::{
                Renderer,
                qr::{Encoder, ErrorCorrection},
            };
            use url::Url;

            let url = Url::parse(format!("{}{}", qr_code_url_prefix, self.service.uuid).as_str())
                .unwrap();
            let symbol = Encoder::new(ErrorCorrection::Medium)
                .encode_to_qr_text(&url)
                .unwrap();
            let renderer = Renderer::new(&symbol, 0);
            info!(%url, "running...\n{:#}", renderer);
        }

        while let Some(event) = self
            .event_rx
            .recv()
            .with_cancellation_token(&cancellation_token)
            .await
        {
            match event {
                None => break,
                Some(PeripheralEvent::StateUpdate { is_powered }) => {
                    self.state_update(is_powered).await
                }
                Some(PeripheralEvent::ReadRequest {
                    request,
                    offset: _,
                    responder,
                }) => {
                    self.read_request(request, responder).await;
                }
                Some(PeripheralEvent::WriteRequest {
                    request,
                    value,
                    offset: _,
                    responder,
                }) => match self.write_request(request, value, responder).await {
                    None => {}
                    Some(configuration_bytes) => return Ok(Some(configuration_bytes)),
                },
                Some(PeripheralEvent::CharacteristicSubscriptionUpdate {
                    request,
                    subscribed,
                }) => {
                    if subscribed {
                        self.subscribed(request).await;
                    } else {
                        self.unsubscribed(request).await;
                    }
                }
            }
        }
        Ok(None)
    }
}
