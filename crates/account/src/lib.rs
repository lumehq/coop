use gpui::{App, AppContext, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};

pub fn init(public_key: PublicKey, cx: &mut App) {
    Account::set_global(cx.new(|cx| Account::new(public_key, cx)), cx);
}

struct GlobalAccount(Entity<Account>);

impl Global for GlobalAccount {}

pub struct Account {
    /// The public key of the account
    public_key: PublicKey,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 1]>,
}

impl Account {
    /// Retrieve the global account state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAccount>().0.clone()
    }

    /// Check if the global account state exists
    pub fn has_global(cx: &App) -> bool {
        cx.has_global::<GlobalAccount>()
    }

    /// Set the global account instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalAccount(state));
    }

    /// Create a new account instance
    pub(crate) fn new(public_key: PublicKey, _cx: &mut Context<Self>) -> Self {
        Self {
            public_key,
            _tasks: smallvec![],
        }
    }

    /// Get the public key of the account
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }
}
