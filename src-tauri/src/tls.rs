//! TLS on Android: rustls with the ring provider and Mozilla's list of root certificates
//! (webpki-roots). The bundled OpenSSL is used there only by SQLCipher, never for the network.
//! On the PC the system's own TLS is used (native-tls - Schannel on Windows).

use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::error::{AppError, AppResult};

fn base() -> AppResult<ClientConfig> {
    let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map(|b| b.with_root_certificates(roots).with_no_client_auth())
        .map_err(|e| AppError::Net(format!("TLS setup: {e}")))
}

/// For the HTTP client (reqwest) - HTTP/2 and HTTP/1.1.
pub fn http_config() -> AppResult<ClientConfig> {
    let mut config = base()?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// A TLS stream to an IMAP server over an open TCP connection.
pub fn imap_stream(host: &str, tcp: TcpStream) -> AppResult<StreamOwned<ClientConnection, TcpStream>> {
    let config = Arc::new(base()?);
    let name = ServerName::try_from(host.to_string()).map_err(|e| AppError::Net(format!("server name: {e}")))?;
    let conn = ClientConnection::new(config, name).map_err(|e| AppError::Net(e.to_string()))?;
    let mut stream = StreamOwned::new(conn, tcp);
    // Handshake now, so a bad certificate is an error here and not at the first IMAP command.
    while stream.conn.is_handshaking() {
        stream.conn.complete_io(&mut stream.sock).map_err(|e| AppError::Net(e.to_string()))?;
    }
    Ok(stream)
}
