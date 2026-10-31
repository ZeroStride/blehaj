use std::{collections::HashMap, sync::Arc};

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
use derive_more::Debug;
#[cfg(feature = "digest")]
use ed25519_dalek::Signer;
#[cfg(feature = "ml-kem")]
use ml_kem::{EncapsulationKey768, KeyExport};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, mpsc::Receiver, oneshot::Sender};
use tokio_util::{future::FutureExt, sync::CancellationToken};
use tracing::{debug, error, info, instrument, trace, warn};
use uuid::{Uuid, uuid};
use zstd::bulk;

use crate::{
    chonker::*,
    configuration::{self},
};

const CONFIGURATION_CHARACTERISTIC_UUID: Uuid = uuid!("4b3bd2c3-2560-4353-8d37-05ca8333d01e");

#[derive(Debug, Deserialize, Serialize)]
enum Request {
    NetworkManagerConfiguration {
        interface: String,

        #[cfg(feature = "ml-kem")]
        #[debug(skip)]
        #[serde(with = "serde_bytes")]
        public_key: Vec<u8>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
struct Envelope {
    pub identifier: String,
    pub algorithm: String,
    pub request: Request,
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

#[derive(Default)]
struct ClientContext {
    pub pending_read_chonks: Option<Vec<BleChonk>>,
    pub pending_write_chonks: Option<Vec<BleChonk>>,
}

pub struct BlePeripheralConfigurationSource {
    // session: bluer::Session,
    // adapter: bluer::Adapter,
    peripheral: Peripheral,
    event_rx: Receiver<PeripheralEvent>,
    service: Service,
    configuration: Arc<configuration::Configuration>,
    context: HashMap<String, ClientContext>,
    chonker: Chonker,
    #[cfg(feature = "ml-kem")]
    encapsulation_key: Arc<EncapsulationKey768>,
}

impl BlePeripheralConfigurationSource {
    #[instrument]
    pub async fn new_from_configuration(
        configuration: Arc<configuration::Configuration>,
        encapsulation_key: Arc<EncapsulationKey768>,
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
                        CharacteristicProperty::WriteWithoutResponse,
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
            context: HashMap::new(),
            chonker: Chonker::new(512),
            encapsulation_key,
        })
    }

    #[instrument(skip(self))]
    async fn state_update(&self, is_powered: bool) {
        debug!("state update");
    }

    #[instrument(skip(self, responder))]
    async fn read_request(
        &mut self,
        request: PeripheralRequest,
        responder: Sender<ReadRequestResponse>,
    ) {
        let context = {
            if !self.context.contains_key(&request.client) {
                self.context
                    .insert(request.client.clone(), ClientContext::default());
            }
            self.context.get_mut(&request.client).unwrap()
        };

        let chonk = if context.pending_read_chonks.is_some()
            && !context.pending_read_chonks.as_ref().unwrap().is_empty()
        {
            context.pending_read_chonks.as_mut().unwrap().pop().unwrap()
        } else {
            let configuration_request = Envelope {
                identifier: Uuid::new_v4().to_string(),
                algorithm: "ed25519".to_owned(),
                request: Request::NetworkManagerConfiguration {
                    #[cfg(feature = "ml-kem")]
                    public_key: self.encapsulation_key.to_bytes().to_vec(),
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

            let value: Vec<u8> = value.try_into().expect("serialization failed");
            let value = bulk::compress(&value, 0).expect("zstd compression failed");
            debug!(len = value.len(), "chonking compressed response");

            let mut chonks = self.chonker.chonk_data(&value);
            let chonk = chonks.pop().unwrap();
            if !chonks.is_empty() {
                context.pending_read_chonks = Some(chonks);
            }
            chonk
        };

        debug!(
            len = &chonk.data.len(),
            is_starting_chonk = &chonk.is_start_of_frame(),
            is_final_chonk = &chonk.is_end_of_frame(),
            "sending chonk"
        );
        trace!(?chonk, "chonk");

        if responder
            .send(ReadRequestResponse {
                value: chonk.try_into().expect("serialization failed"),
                response: RequestResponse::Success,
            })
            .is_err()
        {
            error!("error sending read_request response, receiver dropped");
        }
    }

    #[instrument(skip(self, value, responder))]
    async fn write_request(
        &mut self,
        request: PeripheralRequest,
        value: Vec<u8>,
        responder: Sender<WriteRequestResponse>,
    ) -> Option<Vec<u8>> {
        // debug!("received data from write_request");

        match TryInto::<BleChonk>::try_into(value) {
            Ok(chonk) => {
                // Confirm the write
                if responder
                    .send(WriteRequestResponse {
                        response: RequestResponse::Success,
                    })
                    .is_err()
                {
                    error!("error sending write_request response, receiver dropped");
                }

                let data = if chonk.is_start_of_frame() && chonk.is_end_of_frame() {
                    Some(chonk.data)
                } else {
                    let context = {
                        if !self.context.contains_key(&request.client) {
                            warn!(
                                client = request.client,
                                "no pre-existing client context; this is almost certainly going to result in an encryption error very shortly"
                            );
                            self.context
                                .insert(request.client.clone(), ClientContext::default());
                        }
                        self.context.get_mut(&request.client).unwrap()
                    };

                    let last_chonk = chonk.is_end_of_frame();
                    let chonks: &Vec<BleChonk> = if chonk.is_start_of_frame() {
                        trace!(?chonk, "this is the first chonk");
                        if chonk.is_end_of_frame() {
                            trace!(?chonk, "this is the last chonk");
                            &vec![chonk]
                        } else {
                            // trace!(?chonk, "there are more chonks");
                            context.pending_write_chonks = Some(vec![chonk]);
                            context.pending_write_chonks.as_ref().unwrap()
                        }
                    } else {
                        if last_chonk {
                            trace!(?chonk, "this is the last chonk");
                        } else {
                            trace!(?chonk, "there are more chonks");
                        }
                        let chonks = context.pending_write_chonks.as_mut().unwrap();
                        chonks.push(chonk);
                        chonks
                    };

                    if last_chonk {
                        let compresed: Vec<u8> = chonks.iter().fold(Vec::new(), |mut buffer, c| {
                            trace!(chonk = ?c, "appending");
                            buffer.extend_from_slice(&c.data);
                            buffer
                        });
                        debug!(compressed_size = compresed.len(), "decompressing");
                        Some(compresed)
                    } else {
                        None
                    }
                };

                data.map(|compressed| {
                    // Maximum value willing to allocate is len * 5, because there's no way zstd will hit 5x
                    // but it's a reasonable amount of space to alloc still
                    bulk::decompress(&compressed, compressed.len() * 5)
                        .expect("decompression error")
                })
            }
            Err(e) => {
                error!(%e, "error during dechonking");
                if responder
                    .send(WriteRequestResponse {
                        response: RequestResponse::UnlikelyError,
                    })
                    .is_err()
                {
                    error!("error sending write_request response, receiver dropped");
                }
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

        loop {
            if let Some(event) = self
                .event_rx
                .recv()
                .with_cancellation_token(&cancellation_token)
                .await
            {
                match event {
                    None => break Ok(None),
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
                        Some(configuration_bytes) => break Ok(Some(configuration_bytes)),
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
            } else {
                break Ok(None);
            }
        }
    }
}
