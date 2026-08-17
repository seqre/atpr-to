//! HTTP adapters.
//!
//! Everything here is a thin translation between an HTTP request and the
//! domain, store or resolver beneath it.
//!
//! No protocol *operation* appears in this module — no `XrpcClient`, no
//! `PutRecord`/`ListRecords`/`DeleteRecord`, no `OAuthSession`. Those live in
//! `store` and `resolver`, which is the containment that matters when jacquard
//! makes a breaking change. What does remain is `jacquard_common`'s `Handle`
//! type, used to parse the identity segment of a route: it is the atproto
//! definition of a valid handle, and reimplementing it in `domain` would mean
//! owning syntax rules we do not set.
//!
//! The dependency rule runs one way — `api` → `domain`/`store`/`resolver` —
//! and never back.

pub mod delete;
pub mod error_page;
pub mod info;
pub mod links;
pub mod logout;
pub mod qr;
pub mod resolve;
pub mod shorten;
pub mod shortlink;
pub mod static_files;
pub mod ui;
