use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Duration as CDuration};
use dioxus::prelude::*;
use futures::{
	channel::mpsc::{self, UnboundedSender as Sender},
	StreamExt,
};
use keyring_search::{Limit, List, Search};
use nostr_sdk::prelude::*;

/// The interface for calling a debounce.
///
/// See [`use_debounce`] for more information.
pub struct UseDebounce<T: 'static> {
	sender: Signal<Sender<T>>,
}

impl<T> UseDebounce<T> {
	/// Will start the debounce countdown, resetting it if already started.
	pub fn action(&mut self, data: T) {
		self.sender.write().unbounded_send(data).ok();
	}
}

// Manually implement Clone, Copy, and PartialEq as #[derive] thinks that T needs to implement these (it doesn't).

impl<T> Clone for UseDebounce<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T> Copy for UseDebounce<T> {}

impl<T> PartialEq for UseDebounce<T> {
	fn eq(&self, other: &Self) -> bool {
		self.sender == other.sender
	}
}

/// A hook for allowing a function to be called only after a provided [`Duration`] has passed.
///
/// Once the [`UseDebounce::action`] method is called, a timer will start counting down until
/// the callback is ran. If the [`UseDebounce::action`] method is called again, the timer will restart.
///
/// # Example
///
/// ```rust
/// use dioxus::prelude::*;
/// use dioxus_sdk::utils::timing::use_debounce;
/// use std::time::Duration;
///
/// fn App() -> Element {
///     let mut debounce = use_debounce(Duration::from_millis(2000), |_| println!("ran"));
///
///     rsx! {
///         button {
///             onclick: move |_| {
///                 debounce.action(());
///             },
///             "Click!"
///         }
///     }
/// }
/// ```
pub fn use_debounce<T>(time: Duration, cb: impl FnOnce(T) + Copy + 'static) -> UseDebounce<T> {
	use_hook(|| {
		let (sender, mut receiver) = mpsc::unbounded();
		let debouncer = UseDebounce {
			sender: Signal::new(sender),
		};

		spawn(async move {
			let mut current_task: Option<Task> = None;

			loop {
				if let Some(data) = receiver.next().await {
					if let Some(task) = current_task.take() {
						task.cancel();
					}

					current_task = Some(spawn(async move {
						#[cfg(not(target_family = "wasm"))]
						tokio::time::sleep(time).await;

						#[cfg(target_family = "wasm")]
						gloo_timers::future::sleep(time).await;

						cb(data);
					}));
				}
			}
		});

		debouncer
	})
}

pub fn get_accounts() -> Vec<String> {
	let search = Search::new().expect("Secure Storage is not working.");
	let results = search.by_user("nostr_secret");
	let list = List::list_credentials(&results, Limit::All);
	let accounts: HashSet<String> = list
		.split_whitespace()
		.filter(|v| v.starts_with("npub1"))
		.map(String::from)
		.collect();

	accounts.into_iter().collect()
}

pub fn time_ago(time: Timestamp) -> String {
	let t_now = Timestamp::now().as_u64();
	let t_input = time.as_u64();

	let now = DateTime::from_timestamp(t_now as i64, 0).unwrap();
	let input = DateTime::from_timestamp(t_input as i64, 0).unwrap();

	let diff = now - input;

	if diff < CDuration::hours(24) {
		if diff < CDuration::seconds(60) {
			return " now".to_string();
		} else if diff < CDuration::minutes(60) {
			return format!("{}m", diff.num_minutes());
		} else if diff < CDuration::hours(24) {
			return format!("{}h", diff.num_hours());
		}
	}

	format!("{}d", diff.num_days())
}

pub fn message_time(time: Timestamp) -> String {
	let input = DateTime::from_timestamp(time.as_u64() as i64, 0).unwrap();
	input.format("%H:%M %p").to_string()
}

pub fn is_target(target: &PublicKey, tags: &Vec<Tag>) -> bool {
	for tag in tags {
		if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
			if public_key == target {
				return true;
			}
		}
	}
	false
}
