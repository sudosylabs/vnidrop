use super::*;

impl TargetedTransferModule {
    pub(crate) async fn handle_offer(
        &self,
        remote_endpoint_id: &str,
        challenge: &Challenge,
        offer: SubmitTargetedOffer,
    ) -> WireOfferResponse {
        let authority = self.protocol();
        let expected = saved_device_capabilities().targeted_transfer_protocol_version;
        if authority.inbox.cooldown().is_cooling(remote_endpoint_id) {
            return WireOfferResponse::Refused {
                reason: "identity-cooldown".to_string(),
            };
        }
        if offer.protocol_version != expected {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "protocol-incompatible".to_string(),
            };
        }
        if offer.receiver_endpoint_id != authority.local_endpoint_id {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "receiver-mismatch".to_string(),
            };
        }
        if offer.sender_endpoint_id != remote_endpoint_id {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "sender-mismatch".to_string(),
            };
        }
        if offer.file_count == 0
            || offer.file_count > authority.limits.max_collection_files
            || offer.total_size == 0
            || offer.total_size > authority.limits.max_total_bytes
            || offer.transfer_id.is_empty()
            || offer.manifest_id.is_empty()
            || offer.content_hash.is_empty()
        {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: "manifest-limits".to_string(),
            };
        }
        if let Err(error) = authority
            .limits
            .validate_metadata_text("transfer name", Some(offer.transfer_name.as_str()))
        {
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: error.to_string(),
            };
        }

        if let Ok(Some(existing)) = self.store.get_row(&offer.transfer_id).await {
            if existing.manifest_id != offer.manifest_id
                || existing.content_hash != offer.content_hash
                || existing.file_count != offer.file_count
                || existing.total_size != offer.total_size
                || existing.sender_endpoint_id != offer.sender_endpoint_id
                || existing.receiver_endpoint_id != offer.receiver_endpoint_id
            {
                return WireOfferResponse::Refused {
                    reason: "immutable-transfer-mismatch".to_string(),
                };
            }
            return match existing.state {
                TargetedTransferState::Approved
                | TargetedTransferState::Connecting
                | TargetedTransferState::Transferring
                | TargetedTransferState::Interrupted
                | TargetedTransferState::Completed => WireOfferResponse::Accepted,
                TargetedTransferState::Declined => WireOfferResponse::Declined {
                    reason: "receiver-declined".to_string(),
                },
                TargetedTransferState::Cancelled => WireOfferResponse::Declined {
                    reason: "cancelled".to_string(),
                },
                TargetedTransferState::Failed | TargetedTransferState::Deleted => {
                    WireOfferResponse::Refused {
                        reason: format!("transfer-{}", state_as_str(existing.state)),
                    }
                }
                TargetedTransferState::Preparing
                | TargetedTransferState::Offering
                | TargetedTransferState::AwaitingApproval => WireOfferResponse::Accepted,
            };
        }

        let remote_urls = match parse_offer_relay_urls(&offer.relay_urls) {
            Ok(urls) => urls,
            Err(()) => {
                return WireOfferResponse::Refused {
                    reason: "relay-policy-incompatible".to_string(),
                };
            }
        };
        if !relay_profiles_compatible(
            authority.relay_mode,
            &authority.custom_relay_urls,
            offer.relay_mode,
            &remote_urls,
        ) {
            return WireOfferResponse::Refused {
                reason: "relay-policy-incompatible".to_string(),
            };
        }

        if let Err(error) = authority
            .relationships
            .verify_saved_possession(
                remote_endpoint_id,
                challenge,
                &offer.proof,
                offer.generation,
                offer.relationship_protocol_version,
            )
            .await
        {
            tracing::debug!(error = %error, "targeted offer relationship proof rejected");
            authority
                .inbox
                .cooldown()
                .record_malformed(remote_endpoint_id);
            return WireOfferResponse::Refused {
                reason: if matches!(error, VnidropError::ProtocolIncompatible { .. }) {
                    "protocol-incompatible"
                } else {
                    "unauthenticated"
                }
                .to_string(),
            };
        }

        let pending = PendingTargetedOffer {
            transfer_id: offer.transfer_id,
            sender_endpoint_id: remote_endpoint_id.to_string(),
            receiver_endpoint_id: authority.local_endpoint_id.clone(),
            manifest_id: offer.manifest_id,
            content_hash: offer.content_hash,
            transfer_name: offer.transfer_name,
            file_count: offer.file_count,
            total_size: offer.total_size,
            protocol_version: offer.protocol_version,
            received_at: now_ms(),
        };
        match authority.inbox.submit(pending).await {
            TargetedOfferDecision::Accepted => WireOfferResponse::Accepted,
            TargetedOfferDecision::Declined { reason } => WireOfferResponse::Declined { reason },
            TargetedOfferDecision::Refused { reason } => WireOfferResponse::Refused { reason },
        }
    }

    pub(crate) async fn handle_authorization(
        &self,
        remote_endpoint_id: &str,
        delivery: DeliverTargetedAuthorization,
    ) -> DeliverAuthorizationResponse {
        let authority = self.protocol();
        if authority
            .relationships
            .require_saved(remote_endpoint_id)
            .await
            .is_err()
        {
            return DeliverAuthorizationResponse::Rejected;
        }
        let Ok(auth) = TargetedAuthorization::decode(&delivery.authorization) else {
            return DeliverAuthorizationResponse::Rejected;
        };
        if auth.sender_endpoint_id != remote_endpoint_id
            || auth.receiver_endpoint_id != authority.local_endpoint_id
            || auth.transfer_id != delivery.transfer_id
            || auth.protocol_version
                != saved_device_capabilities().targeted_transfer_protocol_version
        {
            return DeliverAuthorizationResponse::Rejected;
        }
        if auth.parse_bound_blob_ticket().is_err() {
            return DeliverAuthorizationResponse::Rejected;
        }
        if self.persist_authorization(auth).await.is_err() {
            return DeliverAuthorizationResponse::Rejected;
        }
        authority
            .inbox
            .authorization_stored(&delivery.transfer_id)
            .await;
        DeliverAuthorizationResponse::Stored
    }

    pub(crate) async fn handle_cancel(
        &self,
        remote_endpoint_id: &str,
        cancel: CancelTargetedOffer,
    ) -> CancelWireOfferResponse {
        let authority = self.protocol();
        if let Some(pending) = authority.inbox.get_pending(&cancel.transfer_id).await {
            if pending.sender_endpoint_id != remote_endpoint_id {
                return CancelWireOfferResponse::Rejected;
            }
            authority.inbox.discard(&cancel.transfer_id).await;
            return match self.clear_accepted_offer_intent(&cancel.transfer_id).await {
                Ok(()) => CancelWireOfferResponse::Cancelled,
                Err(_) => CancelWireOfferResponse::Rejected,
            };
        }
        if !matches!(
            cancel.terminal,
            Some(TargetedTransferState::Cancelled | TargetedTransferState::Deleted)
        ) {
            return CancelWireOfferResponse::Cancelled;
        }
        match self
            .cancel_from_peer(remote_endpoint_id, &cancel.transfer_id)
            .await
        {
            Ok(true) => CancelWireOfferResponse::Cancelled,
            Ok(false) | Err(_) => CancelWireOfferResponse::Rejected,
        }
    }

    pub(crate) async fn handle_completion(
        &self,
        remote_endpoint_id: &str,
        completion: CompleteTargetedTransfer,
    ) -> CompletionResponse {
        let authority = self.protocol();
        if authority
            .relationships
            .require_saved(remote_endpoint_id)
            .await
            .is_err()
        {
            return CompletionResponse::Rejected;
        }
        let Ok(Some(row)) = self.get_row(&completion.transfer_id).await else {
            return CompletionResponse::Rejected;
        };
        let Ok(auth) = TargetedAuthorization::decode(&completion.authorization) else {
            return CompletionResponse::Rejected;
        };
        if row.role != TargetedTransferRole::Sender
            || row.receiver_endpoint_id != remote_endpoint_id
            || auth.receiver_endpoint_id != remote_endpoint_id
            || auth.sender_endpoint_id != authority.local_endpoint_id
            || auth.transfer_id != row.id
            || auth.protocol_transfer_id != row.protocol_transfer_id
            || auth.manifest_id != row.manifest_id
            || auth.content_hash != row.content_hash
            || auth.transfer_name != row.transfer_name
            || auth.file_count != row.file_count
            || auth.total_size != row.total_size
            || auth.total_size != completion.verified_bytes
            || row.blob_ticket.as_deref() != Some(auth.blob_ticket.as_str())
            || auth.protocol_version
                != saved_device_capabilities().targeted_transfer_protocol_version
        {
            return CompletionResponse::Rejected;
        }
        if matches!(
            row.state,
            TargetedTransferState::Cancelled
                | TargetedTransferState::Declined
                | TargetedTransferState::Failed
                | TargetedTransferState::Deleted
        ) {
            return CompletionResponse::Rejected;
        }
        if !matches!(
            self.authorizations.matches_issued_secret(&row, &auth).await,
            Ok(true)
        ) {
            return CompletionResponse::Rejected;
        }
        if row.state == TargetedTransferState::Completed {
            return CompletionResponse::Recorded;
        }
        match self.mark_sender_completed(&completion.transfer_id).await {
            Ok(_) => CompletionResponse::Recorded,
            Err(_) => CompletionResponse::Rejected,
        }
    }
}
