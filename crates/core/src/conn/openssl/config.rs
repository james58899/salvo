//! openssl module
use std::{fmt::{self, Formatter}, sync::Arc};
use std::fs::File;
use std::io::{Error as IoError, Read, Result as IoResult};
use std::path::Path;

use futures_util::future::{ready, Ready};
use futures_util::stream::{once, Once, Stream};
use openssl::pkey::PKey;
use openssl::ssl::{SslAcceptor, SslAcceptorBuilder, SslMethod, SslRef};
use openssl::x509::X509;
use tokio::io::ErrorKind;

use crate::conn::IntoConfigStream;

/// Private key and certificate
#[derive(Debug)]
pub struct Keycert {
    key: Vec<u8>,
    cert: Vec<u8>,
}

impl Default for Keycert {
    fn default() -> Self {
        Self::new()
    }
}

impl Keycert {
    /// Create a new keycert.
    #[inline]
    pub fn new() -> Self {
        Self {
            key: vec![],
            cert: vec![],
        }
    }
    /// Sets the Tls private key via File Path, returns [`IoError`] if the file cannot be open.
    #[inline]
    pub fn key_from_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
        let mut file = File::open(path.as_ref())?;
        file.read_to_end(&mut self.key)?;
        Ok(self)
    }

    /// Sets the Tls private key via bytes slice.
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = key.into();
        self
    }

    /// Specify the file path for the TLS certificate to use.
    #[inline]
    pub fn cert_from_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
        let mut file = File::open(path.as_ref())?;
        file.read_to_end(&mut self.cert)?;
        Ok(self)
    }

    /// Sets the Tls certificate via bytes slice
    #[inline]
    pub fn with_cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = cert.into();
        self
    }

    /// Get the private key.
    #[inline]
    pub fn key(&mut self) -> IoResult<&[u8]> {
        if self.key.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty key"))
        } else {
            Ok(&self.key)
        }
    }

    /// Get the cert.
    #[inline]
    pub fn cert(&mut self) -> IoResult<&[u8]> {
        if self.cert.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty cert"))
        } else {
            Ok(&self.cert)
        }
    }
}

type BuilderFn = dyn Fn(&mut SslAcceptorBuilder) + Send + Sync + 'static;

/// Builder to set the configuration for the Tls server.
pub struct OpensslConfig {
    keycert: Keycert,
    builder_fn: Option<Arc<BuilderFn>>,
}

impl fmt::Debug for OpensslConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("OpensslConfig").finish()
    }
}

impl OpensslConfig {
    /// Create new `OpensslConfig`
    #[inline]
    pub fn new(keycert: Keycert) -> Self {
        OpensslConfig { keycert, builder_fn: None }
    }

    /// Provide fn to change `SslAcceptorBuilder`
    pub fn set_builder_fn<CB>(&mut self, builder_fn: CB)
    where
        CB: Fn(&mut SslAcceptorBuilder) + Send + Sync + 'static,
    {
        self.builder_fn = Some(Arc::new(builder_fn));
    }

    /// Create [`SslAcceptorBuilder`]
    pub fn create_acceptor_builder(&mut self) -> Result<SslAcceptorBuilder, IoError> {
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;

        let mut certs = X509::stack_from_pem(self.keycert.cert()?)?;
        let mut certs = certs.drain(..);
        builder.set_certificate(
            certs
                .next()
                .ok_or_else(|| IoError::new(ErrorKind::Other, "no leaf certificate"))?
                .as_ref(),
        )?;
        certs.try_for_each(|cert| builder.add_extra_chain_cert(cert))?;
        builder.set_private_key(PKey::private_key_from_pem(self.keycert.key()?)?.as_ref())?;

        // set ALPN protocols
        static PROTOS: &[u8] = b"\x02h2\x08http/1.1";
        builder.set_alpn_protos(PROTOS)?;
        // set uo ALPN selection routine - as select_next_proto
        builder.set_alpn_select_callback(move |_: &mut SslRef, list: &[u8]| {
            openssl::ssl::select_next_proto(PROTOS, list).ok_or(openssl::ssl::AlpnError::NOACK)
        });
        if let Some(builder_fn) = &self.builder_fn {
            builder_fn(&mut builder);
        }
        Ok(builder)
    }
}

impl IntoConfigStream<OpensslConfig> for OpensslConfig {
    type Stream = Once<Ready<OpensslConfig>>;

    fn into_stream(self) -> Self::Stream {
        once(ready(self))
    }
}

impl<T> IntoConfigStream<OpensslConfig> for T
where
    T: Stream<Item = OpensslConfig> + Send + 'static,
{
    type Stream = T;

    fn into_stream(self) -> Self {
        self
    }
}
