use std::time::Duration;

use global::constants::ACCOUNT_IDENTIFIER;
use global::{global_channel, nostr_client, NostrSignal};
use gpui::{App, AppContext, Context, Entity, Global, Window};
use nostr_connect::prelude::*;
use signer_proxy::{BrowserSignerProxy, BrowserSignerProxyOptions};

pub fn init(public_key: PublicKey, window: &mut Window, cx: &mut App) {
    Identity::set_global(cx.new(|cx| Identity::new(public_key, window, cx)), cx);
}

struct GlobalIdentity(Entity<Identity>);

impl Global for GlobalIdentity {}

pub struct Identity {
    public_key: PublicKey,
    nip17_relays: Option<bool>,
    nip65_relays: Option<bool>,
}

impl Identity {
    /// Retrieve the Global Identity instance
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalIdentity>().0.clone()
    }

    /// Retrieve the Identity instance
    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalIdentity>().0.read(cx)
    }

    /// Check if the Global Identity instance has been set
    pub fn has_global(cx: &App) -> bool {
        cx.has_global::<GlobalIdentity>()
    }

    /// Remove the Global Identity instance
    pub fn remove_global(cx: &mut App) {
        cx.remove_global::<GlobalIdentity>();
    }

    /// Set the Global Identity instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalIdentity(state));
    }

    pub(crate) fn new(
        public_key: PublicKey,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            public_key,
            nip17_relays: None,
            nip65_relays: None,
        }
    }

    /// Returns the current identity's public key
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    /// Returns the current identity's NIP-17 relays status
    pub fn nip17_relays(&self) -> Option<bool> {
        self.nip17_relays
    }

    /// Returns the current identity's NIP-65 relays status
    pub fn nip65_relays(&self) -> Option<bool> {
        self.nip65_relays
    }

    /// Starts the browser proxy for nostr signer
    pub fn start_browser_proxy(cx: &App) {
        let proxy = BrowserSignerProxy::new(BrowserSignerProxyOptions::default());
        let url = proxy.url();

        cx.background_spawn(async move {
            let client = nostr_client();
            let channel = global_channel();

            if proxy.start().await.is_ok() {
                webbrowser::open(&url).ok();

                loop {
                    if proxy.is_session_active() {
                        // Save the signer to disk for further logins
                        if let Ok(public_key) = proxy.get_public_key().await {
                            let keys = Keys::generate();
                            let tags = vec![Tag::identifier(ACCOUNT_IDENTIFIER)];
                            let kind = Kind::ApplicationSpecificData;

                            let builder = EventBuilder::new(kind, "extension")
                                .tags(tags)
                                .build(public_key)
                                .sign(&keys)
                                .await;

                            if let Ok(event) = builder {
                                if let Err(e) = client.database().save_event(&event).await {
                                    log::error!("Failed to save event: {e}");
                                };
                            }
                        }

                        // Set the client's signer with current proxy signer
                        client.set_signer(proxy.clone()).await;

                        break;
                    } else {
                        channel.0.send(NostrSignal::ProxyDown).await.ok();
                    }
                    smol::Timer::after(Duration::from_secs(1)).await;
                }
            }
        })
        .detach();
    }
}
