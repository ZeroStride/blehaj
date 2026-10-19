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
use tokio::sync::{mpsc, mpsc::Receiver, oneshot::Sender};
use tokio_util::{future::FutureExt, sync::CancellationToken};
use tracing::{debug, error, info, instrument};
use uuid::{Uuid, uuid};

use crate::configuration::{self};

const CONFIGURATION_CHARACTERISTIC_UUID: Uuid = uuid!("4b3bd2c3-2560-4353-8d37-05ca8333d01e");

pub struct BlePeripheralConfigurationSource {
    // session: bluer::Session,
    // adapter: bluer::Adapter,
    peripheral: Peripheral,
    event_rx: Receiver<PeripheralEvent>,
    service: Service,
    qr_code_url_prefix: Option<String>,
}

impl BlePeripheralConfigurationSource {
    pub async fn new_from_configuration(
        configuration: &configuration::Configuration,
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
            qr_code_url_prefix: configuration.qr_code_url_prefix.clone(),
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
        offset: usize,
        responder: Sender<ReadRequestResponse>,
    ) {
        let value = String::from("ok");
        debug!(value, "sending data to read_request");
        if responder
            .send(ReadRequestResponse {
                value: value.into(),
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
        offset: usize,
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
        Some(value)
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
            .start_advertising("unnamed-todo-add-setting", &[self.service.uuid])
            .await?;

        #[cfg(feature = "qrcode")]
        if let Some(ref qr_code_url_prefix) = self.qr_code_url_prefix {
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
                    offset,
                    responder,
                }) => {
                    self.read_request(request, offset as usize, responder).await;
                }
                Some(PeripheralEvent::WriteRequest {
                    request,
                    value,
                    offset,
                    responder,
                }) => {
                    match self
                        .write_request(request, value, offset as usize, responder)
                        .await
                    {
                        None => {}
                        Some(configuration_bytes) => return Ok(Some(configuration_bytes)),
                    }
                }
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
