//! TLS pinning over real QUIC connections on loopback (spec: peer-transport,
//! "Identity certificates", "Dialing pins the expected key", "Two kinds of
//! connection").

use std::net::SocketAddr;
use std::sync::Arc;

use pairing::Identity;
use quinn::crypto::rustls::QuicClientConfig;
use rand_core::OsRng;
use rustls::client::ResolvesClientCert;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::sign::CertifiedKey;
use transport::endpoint;
use transport::tls::{self, ALPN_MEMBER, ALPN_PAIR, Credentials, Expect, SERVER_NAME};

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

struct Machine {
    identity: Identity,
    credentials: Credentials,
}

fn machine() -> Machine {
    let identity = Identity::generate(&mut OsRng);
    let credentials = Credentials::new(&identity).unwrap();
    Machine {
        identity,
        credentials,
    }
}

/// Dials `server` from a fresh client endpoint; `Ok` with the server's key
/// when the handshake succeeds.
async fn dial(
    client: quinn::ClientConfig,
    server: SocketAddr,
) -> Result<quinn::Connection, quinn::ConnectionError> {
    let endpoint = quinn::Endpoint::client(loopback()).unwrap();
    endpoint
        .connect_with(client, server, SERVER_NAME)
        .unwrap()
        .await
}

/// Accepts connections forever, so a handshake can complete or fail.
fn serve(endpoint: quinn::Endpoint) -> tokio::task::JoinHandle<Vec<Option<model::PeerId>>> {
    tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Some(incoming) = endpoint.accept().await {
            if let Ok(connection) = incoming.await {
                seen.push(tls::connection_peer(&connection));
            }
        }
        seen
    })
}

#[tokio::test]
async fn identity_from_the_certificate_both_ways() {
    let (a, b) = (machine(), machine());
    let server = endpoint::bind(&b.credentials, loopback()).unwrap();
    let addr = server.local_addr().unwrap();
    let accepted = tokio::spawn(async move {
        let connection = server.accept().await.unwrap().await.unwrap();
        tls::connection_peer(&connection)
    });
    let client = endpoint::client(
        &a.credentials,
        Expect::Peer(b.identity.peer_id()),
        ALPN_MEMBER,
    )
    .unwrap();
    let connection = dial(client, addr).await.unwrap();
    assert_eq!(
        tls::connection_peer(&connection),
        Some(b.identity.peer_id())
    );
    assert_eq!(accepted.await.unwrap(), Some(a.identity.peer_id()));
}

#[tokio::test]
async fn wrong_machine_at_a_members_address() {
    let (a, b, x) = (machine(), machine(), machine());
    let server = endpoint::bind(&x.credentials, loopback()).unwrap();
    let addr = server.local_addr().unwrap();
    let served = serve(server.clone());
    let client = endpoint::client(
        &a.credentials,
        Expect::Peer(b.identity.peer_id()),
        ALPN_MEMBER,
    )
    .unwrap();
    assert!(dial(client, addr).await.is_err());
    server.close(0u32.into(), b"");
    assert!(served.await.unwrap().is_empty(), "X saw a connection");
}

#[tokio::test]
async fn pairing_dial_accepts_any_key() {
    let (a, x) = (machine(), machine());
    let server = endpoint::bind(&x.credentials, loopback()).unwrap();
    let addr = server.local_addr().unwrap();
    let _served = serve(server);
    let client = endpoint::client(&a.credentials, Expect::Any, ALPN_PAIR).unwrap();
    let connection = dial(client, addr).await.unwrap();
    assert_eq!(
        tls::connection_peer(&connection),
        Some(x.identity.peer_id())
    );
}

#[tokio::test]
async fn unknown_alpn() {
    let (a, b) = (machine(), machine());
    let server = endpoint::bind(&b.credentials, loopback()).unwrap();
    let addr = server.local_addr().unwrap();
    let _served = serve(server);
    let client = endpoint::client(&a.credentials, Expect::Any, b"h3").unwrap();
    assert!(dial(client, addr).await.is_err());
}

/// Presents one machine's certificate but signs with another's key.
#[derive(Debug)]
struct Stolen(Arc<CertifiedKey>);

impl ResolvesClientCert for Stolen {
    fn resolve(&self, _: &[&[u8]], _: &[rustls::SignatureScheme]) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }

    fn has_certs(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn certificate_without_the_private_key() {
    let (b, x, server_machine) = (machine(), machine(), machine());
    let server = endpoint::bind(&server_machine.credentials, loopback()).unwrap();
    let addr = server.local_addr().unwrap();
    let served = serve(server.clone());

    // X presents B's certificate, signing with X's own key.
    let provider = tls::provider();
    let x_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(x.identity.to_pkcs8_der().to_vec()));
    let signer = provider.key_provider.load_private_key(x_key).unwrap();
    let stolen = Arc::new(CertifiedKey::new(vec![b.credentials.cert.clone()], signer));
    let mut config = tls::client_config(&x.credentials, Expect::Any, ALPN_MEMBER).unwrap();
    config.client_auth_cert_resolver = Arc::new(Stolen(stolen));
    let client = quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(config).unwrap()));

    let result = dial(client, addr).await;
    if let Ok(connection) = &result {
        // The client may finish its side first; the server refuses it.
        let closed = connection.closed().await;
        assert!(matches!(
            closed,
            quinn::ConnectionError::ConnectionClosed(_) | quinn::ConnectionError::TransportError(_)
        ));
    }
    server.close(0u32.into(), b"");
    assert!(
        served.await.unwrap().is_empty(),
        "server accepted a stolen certificate"
    );
}
