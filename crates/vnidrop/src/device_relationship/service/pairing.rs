use super::*;

impl DeviceRelationshipService {
    pub(crate) async fn request_pairing(
        self: &Arc<Self>,
        peer_endpoint_id: String,
    ) -> Result<bool, VnidropError> {
        if self.is_blocked(&peer_endpoint_id).await {
            return Ok(false);
        }
        let peer_lock = self.lock_peer(&peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;

        if let Some(existing) = self.find_row(&peer_endpoint_id).await? {
            if existing.state == DeviceRelationshipState::Saved {
                return Ok(true);
            }
        }

        if !self.can_create_new_relationship(&peer_endpoint_id).await? {
            return Ok(false);
        }

        let Some(taken) = self.eligibility.take_eligibility(&peer_endpoint_id).await? else {
            return Ok(false);
        };
        let generation = self.next_generation(&peer_endpoint_id).await?;
        let now = now_ms();

        // Peer already prompted us for the same qualifying session: merge into
        // one relationship without a second consent prompt.
        if let Some(existing) = self.find_row(&peer_endpoint_id).await? {
            if existing.state == DeviceRelationshipState::PendingIncoming
                && existing.session_id.as_deref() == Some(taken.session_id.as_str())
            {
                self.store
                    .upsert(RelationshipUpsert {
                        remote_endpoint_id: &peer_endpoint_id,
                        state: DeviceRelationshipState::PendingOutgoing,
                        generation,
                        minimum_protocol_version: taken.protocol_version,
                        session_id: Some(&taken.session_id),
                        remote_display_name: taken.remote_display_name.as_deref(),
                        last_authenticated_at: Some(taken.authenticated_at),
                        issued_grant_handle: None,
                        held_grant_handle: None,
                        issued_grant_id: None,
                        held_grant_id: None,
                        peer_ack: false,
                        local_ack: false,
                        created_at: existing.created_at,
                        updated_at: now,
                    })
                    .await?;
                self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::PendingOutgoing);
                drop(_guard);
                self.complete_simultaneous_merge(
                    &peer_endpoint_id,
                    generation,
                    taken.protocol_version,
                )
                .await?;
                return Ok(true);
            }
        }

        self.store
            .upsert(RelationshipUpsert {
                remote_endpoint_id: &peer_endpoint_id,
                state: DeviceRelationshipState::PendingOutgoing,
                generation,
                minimum_protocol_version: taken.protocol_version,
                session_id: Some(&taken.session_id),
                remote_display_name: taken.remote_display_name.as_deref(),
                last_authenticated_at: Some(taken.authenticated_at),
                issued_grant_handle: None,
                held_grant_handle: None,
                issued_grant_id: None,
                held_grant_id: None,
                peer_ack: false,
                local_ack: false,
                created_at: now,
                updated_at: now,
            })
            .await?;
        self.emit_changed(&peer_endpoint_id, DeviceRelationshipState::PendingOutgoing);
        drop(_guard);

        let addr = self.peer_addr(&peer_endpoint_id).await?;
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let response = match tokio::time::timeout(
            self.pairing_timeout,
            client.pairing_request(PairingRequest {
                session_id: taken.session_id.clone(),
                capability: taken.capability.to_vec(),
                protocol_version: taken.protocol_version,
                generation,
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            // Timeout/network failure keeps bounded PendingOutgoing for recovery.
            Ok(Err(_)) | Err(_) => return Ok(true),
        };

        match response {
            PairingRequestResponse::AwaitingConsent { generation } => {
                self.adopt_pending_generation(&peer_endpoint_id, &taken.session_id, generation)
                    .await?;
                Ok(true)
            }
            PairingRequestResponse::Merged { generation } => {
                self.adopt_pending_generation(&peer_endpoint_id, &taken.session_id, generation)
                    .await?;
                self.complete_simultaneous_merge(
                    &peer_endpoint_id,
                    generation,
                    taken.protocol_version,
                )
                .await?;
                Ok(true)
            }
            PairingRequestResponse::AlreadySaved => {
                self.activate_saved(&peer_endpoint_id).await?;
                Ok(true)
            }
            PairingRequestResponse::Rejected => {
                // Keep pending: a transient reject must not erase recoverable state.
                // Explicit decline is handled on the peer via consent=false.
                Ok(true)
            }
        }
    }

    pub(crate) async fn respond_to_pairing(
        self: &Arc<Self>,
        peer_endpoint_id: String,
        accepted: bool,
    ) -> Result<bool, VnidropError> {
        if self.is_blocked(&peer_endpoint_id).await {
            return Ok(false);
        }
        let Some(row) = self.find_row(&peer_endpoint_id).await? else {
            return Ok(false);
        };
        if row.state == DeviceRelationshipState::Saved {
            return Ok(true);
        }
        if row.state != DeviceRelationshipState::PendingIncoming {
            return Ok(false);
        }
        if !accepted {
            if let Some(session_id) = &row.session_id {
                let _ = self
                    .eligibility
                    .consume_session(&peer_endpoint_id, session_id)
                    .await;
            }
            self.delete_relationship(&peer_endpoint_id).await?;
            let addr = self.peer_addr(&peer_endpoint_id).await?;
            let client = RelationshipClient::connect(self.endpoint.clone(), addr);
            let _ = tokio::time::timeout(
                self.pairing_timeout,
                client.pairing_consent(PairingConsent {
                    accepted: false,
                    grant: None,
                    challenge: None,
                    generation: row.generation,
                    protocol_version: row.minimum_protocol_version,
                }),
            )
            .await;
            return Ok(true);
        }

        if let Some(session_id) = &row.session_id {
            let _ = self
                .eligibility
                .consume_session(&peer_endpoint_id, session_id)
                .await;
        }

        let grant = self
            .mint_and_store_issued_grant(
                &peer_endpoint_id,
                row.generation,
                row.minimum_protocol_version,
            )
            .await?;
        let challenge = Challenge::generate();
        let addr = self.peer_addr(&peer_endpoint_id).await?;
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let response = match tokio::time::timeout(
            self.pairing_timeout,
            client.pairing_consent(PairingConsent {
                accepted: true,
                grant: Some(grant.clone()),
                challenge: Some(challenge.encode()),
                generation: row.generation,
                protocol_version: row.minimum_protocol_version,
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            // Leave PendingIncoming so the operator can retry consent.
            Ok(Err(_)) | Err(_) => return Ok(true),
        };

        match response {
            PairingConsentResponse::Completed {
                grant: peer_grant,
                possession_proof,
                ack_challenge,
            } => {
                self.verify_issued_possession(
                    &peer_endpoint_id,
                    &challenge,
                    &possession_proof,
                    row.generation,
                    row.minimum_protocol_version,
                )
                .await?;
                self.store_held_grant(&peer_endpoint_id, &peer_grant)
                    .await?;
                let ack_challenge =
                    Challenge::decode(&ack_challenge).map_err(VnidropError::invalid_input)?;
                let proof = self
                    .prove_held_possession(
                        &peer_endpoint_id,
                        &ack_challenge,
                        peer_grant.generation,
                        peer_grant.protocol_version,
                    )
                    .await?;
                match tokio::time::timeout(
                    self.pairing_timeout,
                    client.pairing_ack(PairingAck {
                        possession_proof: proof,
                        challenge: ack_challenge.encode(),
                        generation: row.generation,
                        protocol_version: row.minimum_protocol_version,
                    }),
                )
                .await
                {
                    Ok(Ok(PairingAckResponse::Acknowledged | PairingAckResponse::AlreadySaved)) => {
                        self.set_acks(&peer_endpoint_id, true, true).await?;
                        self.activate_saved(&peer_endpoint_id).await?;
                        Ok(true)
                    }
                    // Ack lost/failed: grants are stored; pending remains recoverable.
                    Ok(Ok(PairingAckResponse::Rejected)) | Ok(Err(_)) | Err(_) => Ok(true),
                }
            }
            PairingConsentResponse::AlreadySaved => {
                self.activate_saved(&peer_endpoint_id).await?;
                Ok(true)
            }
            PairingConsentResponse::Rejected => Ok(false),
        }
    }

    pub(in crate::device_relationship) async fn handle_pairing_request(
        self: &Arc<Self>,
        remote_endpoint_id: String,
        request: PairingRequest,
    ) -> PairingRequestResponse {
        if self.is_blocked(&remote_endpoint_id).await {
            // Indistinguishable rejection: do not expose block state.
            return PairingRequestResponse::Rejected;
        }
        let peer_lock = self.lock_peer(&remote_endpoint_id).await;
        let _guard = peer_lock.lock().await;
        if request.generation == 0 || i64::try_from(request.generation).is_err() {
            return PairingRequestResponse::Rejected;
        }

        let Ok(existing) = self.find_row(&remote_endpoint_id).await else {
            return PairingRequestResponse::Rejected;
        };
        if let Some(existing) = &existing {
            if existing.state == DeviceRelationshipState::PendingIncoming
                && existing.session_id.as_deref() == Some(request.session_id.as_str())
            {
                let generation = existing.generation.max(request.generation);
                if self
                    .store
                    .set_pending_generation(&remote_endpoint_id, &request.session_id, generation)
                    .await
                    .ok()
                    != Some(true)
                {
                    return PairingRequestResponse::Rejected;
                }
                return PairingRequestResponse::AwaitingConsent { generation };
            }
            // Simultaneous initiation: both sides proved eligibility for the same session.
            if existing.state == DeviceRelationshipState::PendingOutgoing
                && existing.session_id.as_deref() == Some(request.session_id.as_str())
            {
                let generation = existing.generation.max(request.generation);
                let protocol_version = existing.minimum_protocol_version;
                if self
                    .store
                    .set_pending_generation(&remote_endpoint_id, &request.session_id, generation)
                    .await
                    .ok()
                    != Some(true)
                {
                    return PairingRequestResponse::Rejected;
                }
                let should_lead = self.local_endpoint_id.as_str() < remote_endpoint_id.as_str();
                drop(_guard);
                if should_lead {
                    let _ = self
                        .complete_simultaneous_merge(
                            &remote_endpoint_id,
                            generation,
                            protocol_version,
                        )
                        .await;
                }
                return PairingRequestResponse::Merged { generation };
            }
        }

        let capability = match SecretMaterial::new(request.capability) {
            Ok(capability) => capability,
            Err(_) => return PairingRequestResponse::Rejected,
        };
        let local_protocol = saved_device_capabilities().relationship_protocol_version;
        // Peers without a compatible saved-device protocol cannot pair; they
        // retain ordinary invitation flow outside this ALPN.
        if request.protocol_version != local_protocol {
            return PairingRequestResponse::Rejected;
        }
        let local_observation = match self
            .eligibility
            .validate_presented_capability(&remote_endpoint_id, &request.session_id, &capability)
            .await
        {
            Ok(Some(entry)) => Some(entry.remote_display_name),
            Ok(None) | Err(_) => None,
        };
        let Some(remote_display_name) = local_observation else {
            return PairingRequestResponse::Rejected;
        };
        if let Some(existing) = existing.filter(|row| row.state == DeviceRelationshipState::Saved) {
            if existing.session_id.as_deref() == Some(request.session_id.as_str())
                && existing.generation == request.generation
            {
                return PairingRequestResponse::AlreadySaved;
            }
            return PairingRequestResponse::Rejected;
        }

        match self.can_create_new_relationship(&remote_endpoint_id).await {
            Ok(true) => {}
            Ok(false) | Err(_) => return PairingRequestResponse::Rejected,
        }

        let Ok(local_generation) = self.next_generation(&remote_endpoint_id).await else {
            return PairingRequestResponse::Rejected;
        };
        let generation = request.generation.max(local_generation);
        if i64::try_from(generation).is_err() {
            return PairingRequestResponse::Rejected;
        }
        let now = now_ms();
        if self
            .store
            .upsert(RelationshipUpsert {
                remote_endpoint_id: &remote_endpoint_id,
                state: DeviceRelationshipState::PendingIncoming,
                generation,
                minimum_protocol_version: request.protocol_version,
                session_id: Some(&request.session_id),
                remote_display_name: remote_display_name.as_deref(),
                last_authenticated_at: Some(now),
                issued_grant_handle: None,
                held_grant_handle: None,
                issued_grant_id: None,
                held_grant_id: None,
                peer_ack: false,
                local_ack: false,
                created_at: now,
                updated_at: now,
            })
            .await
            .is_err()
        {
            return PairingRequestResponse::Rejected;
        }
        self.emit_changed(
            &remote_endpoint_id,
            DeviceRelationshipState::PendingIncoming,
        );
        PairingRequestResponse::AwaitingConsent { generation }
    }

    async fn next_generation(&self, peer_endpoint_id: &str) -> Result<u64, VnidropError> {
        let generation = self
            .store
            .generation_floor(peer_endpoint_id)
            .await?
            .checked_add(1)
            .filter(|generation| i64::try_from(*generation).is_ok())
            .ok_or_else(|| {
                VnidropError::invalid_input(anyhow::anyhow!("relationship generation exhausted"))
            })?;
        Ok(generation)
    }

    async fn adopt_pending_generation(
        &self,
        peer_endpoint_id: &str,
        session_id: &str,
        generation: u64,
    ) -> Result<(), VnidropError> {
        if i64::try_from(generation).is_err() {
            return Err(VnidropError::invalid_input(anyhow::anyhow!(
                "pairing response did not match the pending relationship"
            )));
        }
        if self
            .store
            .set_pending_generation(peer_endpoint_id, session_id, generation)
            .await?
        {
            return Ok(());
        }
        match self.find_row(peer_endpoint_id).await? {
            Some(row)
                if row.state == DeviceRelationshipState::Saved && row.generation == generation =>
            {
                Ok(())
            }
            _ => Err(VnidropError::invalid_input(anyhow::anyhow!(
                "pairing response did not match the pending relationship"
            ))),
        }
    }

    pub(in crate::device_relationship) async fn handle_pairing_consent(
        self: &Arc<Self>,
        remote_endpoint_id: String,
        consent: PairingConsent,
    ) -> PairingConsentResponse {
        if self.is_blocked(&remote_endpoint_id).await {
            return PairingConsentResponse::Rejected;
        }
        let Ok(Some(row)) = self.find_row(&remote_endpoint_id).await else {
            return PairingConsentResponse::Rejected;
        };
        if consent.protocol_version < row.minimum_protocol_version
            || consent.generation != row.generation
        {
            return PairingConsentResponse::Rejected;
        }
        if row.state == DeviceRelationshipState::Saved {
            return PairingConsentResponse::AlreadySaved;
        }
        if row.state != DeviceRelationshipState::PendingOutgoing {
            return PairingConsentResponse::Rejected;
        }
        if !consent.accepted {
            // Peer declined: clear local pending; eligibility was already consumed by requester.
            let _ = self.delete_relationship(&remote_endpoint_id).await;
            return PairingConsentResponse::Rejected;
        }
        let (Some(peer_grant), Some(challenge_hex)) = (consent.grant, consent.challenge) else {
            return PairingConsentResponse::Rejected;
        };
        let Ok(challenge) = Challenge::decode(&challenge_hex) else {
            return PairingConsentResponse::Rejected;
        };
        if self
            .store_held_grant(&remote_endpoint_id, &peer_grant)
            .await
            .is_err()
        {
            return PairingConsentResponse::Rejected;
        }
        let Ok(local_grant) = self
            .mint_and_store_issued_grant(
                &remote_endpoint_id,
                consent.generation,
                consent.protocol_version,
            )
            .await
        else {
            return PairingConsentResponse::Rejected;
        };
        let Ok(possession_proof) = self
            .prove_held_possession(
                &remote_endpoint_id,
                &challenge,
                peer_grant.generation,
                peer_grant.protocol_version,
            )
            .await
        else {
            return PairingConsentResponse::Rejected;
        };
        let ack_challenge = Challenge::generate();
        if self
            .set_acks(&remote_endpoint_id, false, false)
            .await
            .is_err()
        {
            return PairingConsentResponse::Rejected;
        }
        PairingConsentResponse::Completed {
            grant: Box::new(local_grant),
            possession_proof,
            ack_challenge: ack_challenge.encode(),
        }
    }

    pub(in crate::device_relationship) async fn handle_pairing_ack(
        &self,
        remote_endpoint_id: String,
        ack: PairingAck,
    ) -> PairingAckResponse {
        if self.is_blocked(&remote_endpoint_id).await {
            return PairingAckResponse::Rejected;
        }
        let Ok(Some(row)) = self.find_row(&remote_endpoint_id).await else {
            return PairingAckResponse::Rejected;
        };
        if row.state == DeviceRelationshipState::Saved {
            return PairingAckResponse::AlreadySaved;
        }
        if row.state != DeviceRelationshipState::PendingOutgoing
            && row.state != DeviceRelationshipState::PendingIncoming
        {
            return PairingAckResponse::Rejected;
        }
        // Peer proves possession of our issued grant over the ack challenge we
        // sent in Completed; the challenge travels again on the ack message.
        let Ok(challenge) = Challenge::decode(&ack.challenge) else {
            return PairingAckResponse::Rejected;
        };
        if self
            .verify_issued_possession(
                &remote_endpoint_id,
                &challenge,
                &ack.possession_proof,
                ack.generation,
                ack.protocol_version,
            )
            .await
            .is_err()
        {
            return PairingAckResponse::Rejected;
        }
        if self
            .set_acks(&remote_endpoint_id, true, true)
            .await
            .is_err()
        {
            return PairingAckResponse::Rejected;
        }
        if self.activate_saved(&remote_endpoint_id).await.is_err() {
            return PairingAckResponse::Rejected;
        }
        PairingAckResponse::Acknowledged
    }

    async fn complete_simultaneous_merge(
        self: &Arc<Self>,
        peer_endpoint_id: &str,
        generation: u64,
        protocol_version: u16,
    ) -> Result<(), VnidropError> {
        let peer_lock = self.lock_peer(peer_endpoint_id).await;
        let _guard = peer_lock.lock().await;
        if let Some(row) = self.find_row(peer_endpoint_id).await? {
            if row.state == DeviceRelationshipState::Saved {
                return Ok(());
            }
            // Another merge attempt already minted; do not rotate the issued grant.
            if row.issued_grant_handle.is_some() {
                return Ok(());
            }
        }
        // Deterministic role: smaller endpoint ID leads the consent+ack exchange.
        if self.local_endpoint_id.as_str() >= peer_endpoint_id {
            return Ok(());
        }
        let grant = self
            .mint_and_store_issued_grant(peer_endpoint_id, generation, protocol_version)
            .await?;
        let challenge = Challenge::generate();
        drop(_guard);
        let addr = self.peer_addr(peer_endpoint_id).await?;
        let client = RelationshipClient::connect(self.endpoint.clone(), addr);
        let response = match tokio::time::timeout(
            self.pairing_timeout,
            client.pairing_consent(PairingConsent {
                accepted: true,
                grant: Some(grant),
                challenge: Some(challenge.encode()),
                generation,
                protocol_version,
            }),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) | Err(_) => return Ok(()),
        };
        match response {
            PairingConsentResponse::Completed {
                grant: peer_grant,
                possession_proof,
                ack_challenge,
            } => {
                self.verify_issued_possession(
                    peer_endpoint_id,
                    &challenge,
                    &possession_proof,
                    generation,
                    protocol_version,
                )
                .await?;
                self.store_held_grant(peer_endpoint_id, &peer_grant).await?;
                let ack_challenge =
                    Challenge::decode(&ack_challenge).map_err(VnidropError::invalid_input)?;
                let proof = self
                    .prove_held_possession(
                        peer_endpoint_id,
                        &ack_challenge,
                        peer_grant.generation,
                        peer_grant.protocol_version,
                    )
                    .await?;
                if let Ok(Ok(PairingAckResponse::Acknowledged | PairingAckResponse::AlreadySaved)) =
                    tokio::time::timeout(
                        self.pairing_timeout,
                        client.pairing_ack(PairingAck {
                            possession_proof: proof,
                            challenge: ack_challenge.encode(),
                            generation,
                            protocol_version,
                        }),
                    )
                    .await
                {
                    self.set_acks(peer_endpoint_id, true, true).await?;
                    self.activate_saved(peer_endpoint_id).await?;
                }
            }
            PairingConsentResponse::AlreadySaved => {
                self.activate_saved(peer_endpoint_id).await?;
            }
            PairingConsentResponse::Rejected => {}
        }
        Ok(())
    }
}
