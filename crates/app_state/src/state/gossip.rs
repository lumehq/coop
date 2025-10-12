use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anyhow::{anyhow, Error};
use nostr_sdk::prelude::*;

use crate::constants::BOOTSTRAP_RELAYS;
use crate::state::SignalKind;
use crate::{app_state, nostr_client};

#[derive(Debug, Clone, Default)]
pub struct Gossip {
    pub nip17: HashMap<PublicKey, HashSet<RelayUrl>>,
    pub nip65: HashMap<PublicKey, HashSet<(RelayUrl, Option<RelayMetadata>)>>,
}

impl Gossip {
    /// Parse and insert NIP-65 or NIP-17 relays into the gossip state.
    pub fn insert(&mut self, event: &Event) {
        match event.kind {
            Kind::InboxRelays => {
                let urls: Vec<RelayUrl> =
                    nip17::extract_relay_list(event).take(3).cloned().collect();

                if !urls.is_empty() {
                    self.nip17.entry(event.pubkey).or_default().extend(urls);
                }
            }
            Kind::RelayList => {
                let urls: Vec<(RelayUrl, Option<RelayMetadata>)> = nip65::extract_relay_list(event)
                    .map(|(url, metadata)| (url.to_owned(), metadata.to_owned()))
                    .collect();

                if !urls.is_empty() {
                    self.nip65.entry(event.pubkey).or_default().extend(urls);
                }
            }
            _ => {}
        }
    }

    /// Get all write relays for a given public key
    pub fn write_relays(&self, public_key: &PublicKey) -> Vec<&RelayUrl> {
        self.nip65
            .get(public_key)
            .map(|relays| {
                relays
                    .iter()
                    .filter(|(_, metadata)| metadata.as_ref() == Some(&RelayMetadata::Write))
                    .map(|(url, _)| url)
                    .take(3)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all read relays for a given public key
    pub fn read_relays(&self, public_key: &PublicKey) -> Vec<&RelayUrl> {
        self.nip65
            .get(public_key)
            .map(|relays| {
                relays
                    .iter()
                    .filter(|(_, metadata)| metadata.as_ref() == Some(&RelayMetadata::Read))
                    .map(|(url, _)| url)
                    .take(3)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all messaging relays for a given public key
    pub fn messaging_relays(&self, public_key: &PublicKey) -> Vec<&RelayUrl> {
        self.nip17
            .get(public_key)
            .map(|relays| relays.iter().collect())
            .unwrap_or_default()
    }

    /// Get and verify NIP-65 relays for a given public key
    ///
    /// Only fetch from the public relays
    pub async fn get_nip65(&self, public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let timeout = Duration::from_secs(5);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let latest_filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .limit(1);

        // Subscribe to events from the bootstrapping relays
        client
            .subscribe_to(BOOTSTRAP_RELAYS, latest_filter.clone(), Some(opts))
            .await?;

        let filter = Filter::new()
            .kind(Kind::RelayList)
            .author(public_key)
            .since(Timestamp::now());

        // Continuously subscribe for new events from the bootstrap relays
        client
            .subscribe_to(BOOTSTRAP_RELAYS, filter.clone(), Some(opts))
            .await?;

        // Verify the received data after a timeout
        smol::spawn(async move {
            smol::Timer::after(timeout).await;

            if client.database().count(latest_filter).await.unwrap_or(0) < 1 {
                app_state()
                    .signal
                    .send(SignalKind::GossipRelaysNotFound)
                    .await;
            }
        })
        .detach();

        Ok(())
    }

    /// Set NIP-65 relays for a current user
    pub async fn set_nip65(
        &mut self,
        relays: &[(RelayUrl, Option<RelayMetadata>)],
    ) -> Result<(), Error> {
        let client = nostr_client();
        let signer = client.signer().await?;

        let tags: Vec<Tag> = relays
            .iter()
            .map(|(url, metadata)| Tag::relay_metadata(url.to_owned(), metadata.to_owned()))
            .collect();

        let event = EventBuilder::new(Kind::RelayList, "")
            .tags(tags)
            .sign(&signer)
            .await?;

        // Send event to the public relays
        client.send_event_to(BOOTSTRAP_RELAYS, &event).await?;

        // Update gossip data
        for relay in relays {
            self.nip65
                .entry(event.pubkey)
                .or_default()
                .insert(relay.to_owned());
        }

        // Get NIP-17 relays
        self.get_nip17(event.pubkey).await?;

        Ok(())
    }

    /// Get and verify NIP-17 relays for a given public key
    ///
    /// Only fetch from public key's write relays
    pub async fn get_nip17(&self, public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let timeout = Duration::from_secs(5);
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let urls = self.write_relays(&public_key);

        // Ensure user's have at least one write relay
        if urls.is_empty() {
            return Err(anyhow!("NIP-17 relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        let latest_filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(public_key)
            .limit(1);

        // Subscribe to events from the bootstrapping relays
        client
            .subscribe_to(urls.clone(), latest_filter.clone(), Some(opts))
            .await?;

        let filter = Filter::new()
            .kind(Kind::InboxRelays)
            .author(public_key)
            .since(Timestamp::now());

        // Continuously subscribe for new events from the bootstrap relays
        client
            .subscribe_to(urls, filter.clone(), Some(opts))
            .await?;

        // Verify the received data after a timeout
        smol::spawn(async move {
            smol::Timer::after(timeout).await;

            if client.database().count(latest_filter).await.unwrap_or(0) < 1 {
                app_state()
                    .signal
                    .send(SignalKind::MessagingRelaysNotFound)
                    .await;
            }
        })
        .detach();

        Ok(())
    }

    /// Set NIP-17 relays for a current user
    pub async fn set_nip17(&mut self, relays: &[RelayUrl]) -> Result<(), Error> {
        let client = nostr_client();
        let signer = client.signer().await?;
        let public_key = signer.get_public_key().await?;

        let urls = self.write_relays(&public_key);

        // Ensure user's have at least one relay
        if urls.is_empty() {
            return Err(anyhow!("Relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        let event = EventBuilder::new(Kind::InboxRelays, "")
            .tags(relays.iter().map(|relay| Tag::relay(relay.to_owned())))
            .sign(&signer)
            .await?;

        // Send event to the public relays
        client.send_event_to(urls, &event).await?;

        // Update gossip data
        for relay in relays {
            self.nip17
                .entry(event.pubkey)
                .or_default()
                .insert(relay.to_owned());
        }

        // Run inbox monitor
        self.monitor_inbox(event.pubkey).await?;

        Ok(())
    }

    /// Subscribe for events that match the given kind for a given author
    ///
    /// Only fetch from author's write relays
    pub async fn subscribe(&self, public_key: PublicKey, kind: Kind) -> Result<(), Error> {
        let client = nostr_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let filter = Filter::new().author(public_key).kind(kind).limit(1);
        let urls = self.write_relays(&public_key);

        // Ensure user's have at least one write relay
        if urls.is_empty() {
            return Err(anyhow!("NIP-65 relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Subscribe to filters to user's write relays
        client.subscribe_to(urls, filter, Some(opts)).await?;

        Ok(())
    }

    /// Bulk subscribe to metadata events for a list of public keys
    ///
    /// Only fetch from the public relays
    pub async fn bulk_subscribe(&self, public_keys: HashSet<PublicKey>) -> Result<(), Error> {
        if public_keys.is_empty() {
            return Err(anyhow!("You need at least one public key"));
        }

        let client = nostr_client();
        let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::ExitOnEOSE);

        let kinds = vec![Kind::Metadata, Kind::ContactList, Kind::RelayList];
        let limit = public_keys.len() * kinds.len() + 20;

        let filter = Filter::new().authors(public_keys).kinds(kinds).limit(limit);
        let urls = BOOTSTRAP_RELAYS;

        // Subscribe to filters to the bootstrap relays
        client.subscribe_to(urls, filter, Some(opts)).await?;

        Ok(())
    }

    /// Monitor all gift wrap events in the messaging relays for a given public key
    pub async fn monitor_inbox(&self, public_key: PublicKey) -> Result<(), Error> {
        let client = nostr_client();
        let id = SubscriptionId::new("inbox");
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(public_key);
        let urls = self.messaging_relays(&public_key);

        // Ensure user's have at least one messaging relay
        if urls.is_empty() {
            return Err(anyhow!("Messaging relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Subscribe to filters to user's messaging relays
        client.subscribe_with_id_to(urls, id, filter, None).await?;

        Ok(())
    }

    /// Send an event to author's write relays
    pub async fn send_event_to_write_relays(&self, event: &Event) -> Result<(), Error> {
        let client = nostr_client();
        let public_key = event.pubkey;
        let urls = self.write_relays(&public_key);

        // Ensure user's have at least one relay
        if urls.is_empty() {
            return Err(anyhow!("Relays are empty"));
        }

        // Ensure connection to relays
        for url in urls.iter().cloned() {
            client.add_relay(url).await?;
            client.connect_relay(url).await?;
        }

        // Send event to relays
        client.send_event(event).await?;

        Ok(())
    }
}
