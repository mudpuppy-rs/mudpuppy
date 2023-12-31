use std::fmt::{Display, Formatter};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use pyo3::{pyclass, pymethods};
use socket2::{Socket, TcpKeepalive};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use tracing::{info, instrument, Level};

use crate::error::Error;
use crate::model::{Mud, Tls};

/// A TCP stream to a MUD server that may be TLS encrypted.
#[derive(Debug)]
pub enum Stream {
    /// A vanilla TCP stream.
    Tcp(TcpStream),

    /// A TLS encrypted TCP stream, with an indication of whether certificate verification was
    /// performed.
    Tls {
        tls_stream: TlsStream<TcpStream>,
        verify_skipped: bool,
    },
}

impl Stream {
    #[instrument(level = Level::TRACE, skip(mud))]
    pub async fn connect(mud: &Mud) -> Result<Stream, Error> {
        info!("connecting");
        let mut tcp_stream = happy_eyeballs::tokio::connect((mud.host.as_str(), mud.port)).await?;

        if !mud.no_tcp_keepalive {
            tcp_stream = Self::configure_keepalive(tcp_stream)?;
        }

        let ip_addr = tcp_stream
            .peer_addr()
            .map(|addr| addr.ip().to_string())
            .unwrap_or_default();
        info!("connected to {ip_addr}:{}", mud.port);

        Ok(match mud.tls {
            Tls::Disabled => Stream::Tcp(tcp_stream),
            Tls::Enabled | Tls::InsecureSkipVerify => Stream::Tls {
                tls_stream: Self::connect_tls(mud, tcp_stream).await?,
                verify_skipped: mud.tls == Tls::InsecureSkipVerify,
            },
        })
    }

    // TODO(XXX): support choosing crypto provider?
    // TODO(XXX): use rustls-platform-verifier.
    async fn connect_tls(mud: &Mud, tcp_stream: TcpStream) -> Result<TlsStream<TcpStream>, Error> {
        let config = match mud.tls {
            Tls::Enabled => ClientConfig::builder().with_root_certificates(RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.into(),
            }),
            Tls::InsecureSkipVerify => ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(
                    danger::NoCertificateVerification::new(),
                )),
            Tls::Disabled => unreachable!("connect_tls should not be called with Tls::None"),
        };

        TlsConnector::from(Arc::new(config.with_no_client_auth()))
            .connect(
                // Safety: config verifiers mud host up-front.
                ServerName::try_from(mud.host.as_str()).unwrap().to_owned(),
                tcp_stream,
            )
            .await
            .map_err(Into::into)
    }

    fn configure_keepalive(tcp_stream: TcpStream) -> Result<TcpStream, Error> {
        // Convert the Tokio TCP stream into a std::net::TcpStream, and then a socket2::Socket.
        let tcp_stream = tcp_stream.into_std()?;
        let sock = Socket::from(tcp_stream);

        // Configure the TCP keepalive behaviour of the socket.
        //
        // Values are loosely based on Mudlet's settings, but tuned to be a little more aggressive.
        // E.g. a shorter wait before sending keepalives, a shorter wait between keepalives, and
        // fewer retries before giving up.
        // https://github.com/Mudlet/Mudlet/blob/31ea3079e63735a344379e714117e4f1ad6b2f1b/src/ctelnet.cpp#L3052-L3138
        #[allow(unused_mut)]
        let mut keepalive = TcpKeepalive::new()
            // How long will the connection be allowed to sit idle before the first keepalive
            // packet is sent?
            .with_time(Duration::from_secs(30))
            // How long should we wait between sending keepalive packets?
            .with_interval(Duration::from_secs(5));

        #[cfg(not(target_os = "windows"))]
        {
            // How many keepalive packets should we send before deciding a connection is dead?
            keepalive = keepalive.with_retries(5);
        }

        sock.set_tcp_keepalive(&keepalive)?;

        // Convert the socket back into a std TCP stream, and then a Tokio TCP stream.
        let tcp_stream: std::net::TcpStream = sock.into();
        TcpStream::from_std(tcp_stream).map_err(Into::into)
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Tcp(tcp_stream) => Pin::new(tcp_stream).poll_read(cx, buf),
            Stream::Tls { tls_stream, .. } => Pin::new(tls_stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.get_mut() {
            Stream::Tcp(tcp_stream) => Pin::new(tcp_stream).poll_write(cx, buf),
            Stream::Tls { tls_stream, .. } => Pin::new(tls_stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            Stream::Tcp(tcp_stream) => Pin::new(tcp_stream).poll_flush(cx),
            Stream::Tls { tls_stream, .. } => Pin::new(tls_stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            Stream::Tcp(tcp_stream) => Pin::new(tcp_stream).poll_shutdown(cx),
            Stream::Tls { tls_stream, .. } => Pin::new(tls_stream).poll_shutdown(cx),
        }
    }
}

impl From<&Stream> for Info {
    fn from(value: &Stream) -> Self {
        fn ip_and_port(stream: &TcpStream) -> (String, u16) {
            stream
                .peer_addr()
                .map(|addr| (addr.ip().to_string(), addr.port()))
                .unwrap_or_default()
        }
        match value {
            Stream::Tcp(stream) => {
                let (ip, port) = ip_and_port(stream);
                Info::Tcp { ip, port }
            }
            Stream::Tls {
                tls_stream,
                verify_skipped,
            } => {
                let (tcp_stream, tls_conn) = tls_stream.get_ref();
                let (ip, port) = ip_and_port(tcp_stream);
                Info::Tls {
                    ip,
                    port,
                    protocol: tls_conn
                        .protocol_version()
                        .map(|proto| proto.as_str().unwrap_or_default().into())
                        .unwrap_or_default(),
                    ciphersuite: tls_conn
                        .negotiated_cipher_suite()
                        .map(|cs| cs.suite().as_str().unwrap_or_default().into())
                        .unwrap_or_default(),
                    verify_skipped: *verify_skipped,
                }
            }
        }
    }
}

/// A description of the stream's connection information.
#[derive(Clone, Debug, Eq, PartialEq)]
#[pyclass(name = "StreamInfo")]
pub enum Info {
    /// The stream is an unencrypted TCP stream.
    Tcp {
        /// The resolved IP address of the MUD server that was used for the connection stream.
        ip: String,
        /// The port of the MUD server that was used for the connection stream.
        port: u16,
    },

    /// The stream is a TLS encrypted TCP stream.
    Tls {
        /// The resolved IP address of the MUD server that was used for the connection stream.
        ip: String,
        /// The port of the MUD server that was used for the connection stream.
        port: u16,
        /// The TLS protocol name.
        protocol: String,
        /// The TLS ciphersuite name.
        ciphersuite: String,
        /// Whether or not certificate verification was skipped (insecure).
        verify_skipped: bool,
    },
}

#[pymethods]
impl Info {
    fn __str__(&self) -> String {
        format!("{self}")
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

impl Display for Info {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Info::Tcp { ip, port } => {
                write!(f, "telnet://{ip}:{port}")
            }
            Info::Tls {
                ip,
                port,
                protocol,
                ciphersuite,
                verify_skipped,
            } => {
                write!(
                    f,
                    "tls://{ip}:{port} ({protocol} {ciphersuite}{})",
                    if *verify_skipped {
                        " !verify-skipped!"
                    } else {
                        ""
                    },
                )
            }
        }
    }
}

mod danger {
    use tokio_rustls::rustls::client::danger::{
        HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
    };
    use tokio_rustls::rustls::crypto::ring::default_provider;
    use tokio_rustls::rustls::crypto::{
        verify_tls12_signature, verify_tls13_signature, CryptoProvider,
    };
    use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use tokio_rustls::rustls::{DigitallySignedStruct, Error, SignatureScheme};

    /// `NoCertificateVerification` is a **DANGEROUS** [`ServerCertVerifier`] that
    /// performs **no** certificate validation.
    #[derive(Debug)]
    pub struct NoCertificateVerification(CryptoProvider);

    impl NoCertificateVerification {
        pub fn new() -> Self {
            NoCertificateVerification::default()
        }
    }

    impl Default for NoCertificateVerification {
        fn default() -> Self {
            Self(default_provider())
        }
    }

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            verify_tls12_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            verify_tls13_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }
}
