use std::fmt;

use anyhow::Result;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
    Endpoint, EndpointAddr,
};
use irpc::{channel::oneshot, rpc_requests, Client, WithChannels};
use irpc_iroh::{read_request, IrohLazyRemoteConnection};
use serde::{Deserialize, Serialize};

use crate::api::TransferMetadata;

#[derive(Clone)]
pub(crate) struct HandshakeService {
    approval: crate::approval::ApprovalService,
}

impl fmt::Debug for HandshakeService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HandshakeService")
    }
}

impl HandshakeService {
    pub(crate) const ALPN: &'static [u8] = b"/vnidrop/handshake/2";

    pub(crate) fn new(approval: crate::approval::ApprovalService) -> Self {
        Self { approval }
    }

    pub(crate) fn client(endpoint: Endpoint, addr: EndpointAddr) -> HandshakeClient {
        HandshakeClient {
            inner: Client::boxed(IrohLazyRemoteConnection::new(
                endpoint,
                addr,
                Self::ALPN.to_vec(),
            )),
        }
    }

    async fn handle_request(
        &self,
        remote_endpoint_id: String,
        request: RequestTransfer,
    ) -> HandshakeResponse {
        self.approval
            .request_transfer(remote_endpoint_id, request)
            .await
    }
}

impl ProtocolHandler for HandshakeService {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();

        while let Some(message) = read_request::<HandshakeProtocol>(&connection).await? {
            match message {
                HandshakeMessage::RequestTransfer(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    // The receiver-provided name is display data. The trusted
                    // identity is the endpoint id from the Iroh connection.
                    let response = self.handle_request(remote_endpoint_id.clone(), inner).await;
                    let _ = tx.send(response).await;
                }
                HandshakeMessage::ReportDelivery(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .approval
                        .complete_delivery(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
                HandshakeMessage::ReportDeliveryFailure(message) => {
                    let WithChannels { inner, tx, .. } = message;
                    let response = self
                        .approval
                        .fail_delivery(remote_endpoint_id.clone(), inner)
                        .await;
                    let _ = tx.send(response).await;
                }
            }
        }

        connection.closed().await;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HandshakeClient {
    inner: Client<HandshakeProtocol>,
}

impl HandshakeClient {
    pub(crate) async fn request_transfer(
        &self,
        metadata: &TransferMetadata,
        receiver_name: Option<&str>,
    ) -> Result<HandshakeResponse, irpc::Error> {
        self.inner
            .rpc(RequestTransfer {
                transfer_id: metadata.transfer_id,
                transfer_hash: metadata.content_hash.clone(),
                transfer_name: metadata.transfer_name.clone(),
                receiver_name: receiver_name.map(ToOwned::to_owned),
                receiver_device_name: None,
                app_version: env!("CARGO_PKG_VERSION").to_string(),
            })
            .await
    }

    pub(crate) async fn report_delivery(
        &self,
        receipt: DeliveryReceipt,
    ) -> Result<DeliveryReceiptResponse, irpc::Error> {
        self.inner.rpc(receipt).await
    }

    pub(crate) async fn report_delivery_failure(
        &self,
        receipt: DeliveryFailureReceipt,
    ) -> Result<DeliveryReceiptResponse, irpc::Error> {
        self.inner.rpc(receipt).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestTransfer {
    pub(crate) transfer_id: u64,
    pub(crate) transfer_hash: String,
    pub(crate) transfer_name: String,
    pub(crate) receiver_name: Option<String>,
    pub(crate) receiver_device_name: Option<String>,
    pub(crate) app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum HandshakeResponse {
    Approved {
        request_id: String,
        token: String,
        expires_at: i64,
    },
    Denied {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeliveryReceipt {
    pub(crate) request_id: String,
    pub(crate) transfer_id: u64,
    pub(crate) token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeliveryFailureReceipt {
    pub(crate) request_id: String,
    pub(crate) transfer_id: u64,
    pub(crate) token: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum DeliveryReceiptResponse {
    Recorded,
    Rejected { reason: String },
}

#[rpc_requests(message = HandshakeMessage)]
#[derive(Debug, Serialize, Deserialize)]
enum HandshakeProtocol {
    #[rpc(tx=oneshot::Sender<HandshakeResponse>)]
    RequestTransfer(RequestTransfer),
    #[rpc(tx=oneshot::Sender<DeliveryReceiptResponse>)]
    ReportDelivery(DeliveryReceipt),
    #[rpc(tx=oneshot::Sender<DeliveryReceiptResponse>)]
    ReportDeliveryFailure(DeliveryFailureReceipt),
}
