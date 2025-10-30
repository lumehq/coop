use std::collections::HashMap;

use gpui::{App, AppContext, AsyncApp, Context, Entity, Global, Task};
use nostr_sdk::prelude::*;
use smallvec::{smallvec, SmallVec};
use states::app_state;

pub fn init(cx: &mut App) {
    PersonRegistry::set_global(cx.new(PersonRegistry::new), cx);
}

struct GlobalPersonRegistry(Entity<PersonRegistry>);

impl Global for GlobalPersonRegistry {}

pub struct PersonRegistry {
    /// Collection of all persons (user profiles)
    pub persons: HashMap<PublicKey, Entity<Profile>>,

    /// Tasks for asynchronous operations
    _tasks: SmallVec<[Task<()>; 2]>,
}

impl PersonRegistry {
    /// Retrieve the global person registry state
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalPersonRegistry>().0.clone()
    }

    /// Set the global person registry instance
    pub(crate) fn set_global(state: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalPersonRegistry(state));
    }

    /// Create a new person registry instance
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let mut tasks = smallvec![];

        tasks.push(
            // Load all user profiles from the database
            cx.spawn(async move |this, cx| {
                match Self::load_persons(cx).await {
                    Ok(profiles) => {
                        this.update(cx, |this, cx| {
                            this.bulk_insert_persons(profiles, cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        log::error!("Failed to load persons: {e}");
                    }
                };
            }),
        );

        Self {
            persons: HashMap::new(),
            _tasks: tasks,
        }
    }

    /// Create a task to load all user profiles from the database
    fn load_persons(cx: &AsyncApp) -> Task<Result<Vec<Profile>, Error>> {
        cx.background_spawn(async move {
            let client = app_state().client();
            let filter = Filter::new().kind(Kind::Metadata).limit(200);
            let events = client.database().query(filter).await?;

            let mut profiles = vec![];

            for event in events.into_iter() {
                let metadata = Metadata::from_json(event.content).unwrap_or_default();
                let profile = Profile::new(event.pubkey, metadata);
                profiles.push(profile);
            }

            Ok(profiles)
        })
    }

    /// Insert batch of persons
    fn bulk_insert_persons(&mut self, profiles: Vec<Profile>, cx: &mut Context<Self>) {
        for profile in profiles.into_iter() {
            self.persons
                .insert(profile.public_key(), cx.new(|_| profile));
        }
        cx.notify();
    }

    /// Insert or update a person
    pub fn insert_or_update_person(&mut self, profile: Profile, cx: &mut App) {
        let public_key = profile.public_key();

        match self.persons.get(&public_key) {
            Some(person) => {
                person.update(cx, |this, cx| {
                    *this = profile;
                    cx.notify();
                });
            }
            None => {
                self.persons.insert(public_key, cx.new(|_| profile));
            }
        }
    }

    /// Get single person
    pub fn get_person(&self, public_key: &PublicKey, cx: &App) -> Profile {
        self.persons
            .get(public_key)
            .map(|e| e.read(cx))
            .cloned()
            .unwrap_or(Profile::new(public_key.to_owned(), Metadata::default()))
    }

    /// Get group of persons
    pub fn get_group_person(&self, public_keys: &[PublicKey], cx: &App) -> Vec<Profile> {
        let mut profiles = vec![];

        for public_key in public_keys.iter() {
            let profile = self.get_person(public_key, cx);
            profiles.push(profile);
        }

        profiles
    }
}
