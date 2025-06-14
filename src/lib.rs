// Tests use std as it's easier
#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![forbid(unsafe_code)]
// avoids headscratching
#![deny(unused_must_use)]
// for the time being silence the lint. need to figure if we actually
// want to work with Send (not needed for embassy)
#![allow(async_fn_in_trait)]
// XXX unused_imports only during dev churn
#![allow(unused_imports)]

pub mod config;
pub mod packets;
pub mod sshnames;
pub mod sshwire;
// exported so that some Channel error variants can be created with .fail().
// perhaps the ones of interest should be expored separately.
pub mod error;
pub mod random;

pub mod event;

mod conn;
mod encrypt;
mod ident;
mod kex;
mod namelist;
mod sign;
mod test;

mod cliauth;
mod client;

mod servauth;
mod server;

mod auth;
mod channel;
mod runner;
mod ssh_chapoly;
mod sunsetlog;
mod termmodes;
mod traffic;

use conn::DispatchEvent;
use event::{CliEventId, ServEventId};

// Application API
pub use sshwire::TextString;

pub use auth::AuthSigMsg;
pub use channel::{ChanData, ChanNum, CliSessionExit};
pub use channel::{ChanOpened, Pty, SessionCommand};
pub use error::{Error, Result};
pub use packets::{PubKey, Signature};
pub use sign::{KeyType, OwnedSig, SignKey};
pub use sshnames::ChanFail;

pub use event::{CliEvent, Event, ServEvent};
pub use runner::ChanHandle;
pub use runner::Runner;
