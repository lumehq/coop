use nostr_connect::prelude::*;

#[derive(Debug, Clone)]
pub struct CoopAuthUrlHandler;

impl AuthUrlHandler for CoopAuthUrlHandler {
    fn on_auth_url(&self, auth_url: Url) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            webbrowser::open(auth_url.as_str())?;
            Ok(())
        })
    }
}
