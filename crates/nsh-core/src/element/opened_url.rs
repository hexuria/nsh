//! The URLs the system opens the app with, for views
//! ([`super::ambient::on_open_url`]): a link to the app's scheme clicked in
//! another app, `open "app://…"` in a terminal, the URL that launched it.
//!
//! The platform hands each to the engine, which gives it to the core
//! ([`crate::Core::url_opened`]): every handler a view registered runs with
//! it, in the order they were added. A URL that comes while no view
//! listens (it launched the app, and the root has not run yet) waits, up to
//! [`MAX_PENDING`], for the first handler, which gets it in its first turn.

use std::rc::Rc;

/// How many URLs wait for a handler at most; then the oldest go.
pub(crate) const MAX_PENDING: usize = 32;

/// A view's handler of opened URLs.
pub(crate) type OpenUrlHandler = Rc<dyn Fn(&str)>;

/// The views' handlers, oldest first, and the URLs no handler took yet.
#[derive(Default)]
pub(crate) struct OpenedUrls {
    next: u64,
    pub(crate) handlers: Vec<(u64, OpenUrlHandler)>,
    pending: Vec<String>,
}

impl OpenedUrls {
    /// Adds a handler; its id.
    pub(crate) fn add(&mut self, handler: OpenUrlHandler) -> u64 {
        self.next += 1;
        self.handlers.push((self.next, handler));
        self.next
    }

    /// Removes handler `id`; returns it so it is dropped outside the borrow
    /// (it may own what removes others).
    pub(crate) fn remove(&mut self, id: u64) -> Option<OpenUrlHandler> {
        let i = self.handlers.iter().position(|(i, _)| *i == id)?;
        Some(self.handlers.remove(i).1)
    }

    /// The handlers `url` goes to now, or none: then it waits for the first.
    pub(crate) fn route(&mut self, url: String) -> Vec<OpenUrlHandler> {
        if self.handlers.is_empty() {
            if self.pending.len() == MAX_PENDING {
                self.pending.remove(0);
            }
            self.pending.push(url);
            return Vec::new();
        }
        self.handlers.iter().map(|(_, h)| h.clone()).collect()
    }

    /// The URLs that waited for a handler, oldest first.
    pub(crate) fn take_pending(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_wait_for_the_first_handler() {
        let mut urls = OpenedUrls::default();
        for i in 0..MAX_PENDING + 2 {
            assert!(urls.route(format!("app://{i}")).is_empty());
        }
        let pending = urls.take_pending();
        assert_eq!(pending.len(), MAX_PENDING);
        assert_eq!(pending[0], "app://2");
        let id = urls.add(Rc::new(|_| {}));
        assert_eq!(urls.route("app://x".into()).len(), 1);
        assert!(urls.take_pending().is_empty());
        assert!(urls.remove(id).is_some());
        assert!(urls.route("app://y".into()).is_empty());
        assert_eq!(urls.take_pending(), vec!["app://y".to_owned()]);
    }
}
