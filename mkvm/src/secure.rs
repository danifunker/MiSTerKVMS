//! Encrypted mkvm: every viewer connection (docs/IPHONE.md §3).
//!
//! 1. The viewer opens with [`MAGIC_NX`], [`VERSION`] and a guard byte, then
//!    a Noise `NX_25519_ChaChaPoly_BLAKE2s` handshake: one round trip, no key
//!    of its own. The capture PC proves its long-term identity
//!    ([`ServerIdentity`]); viewers remember it per host. Each connection
//!    gets fresh session keys (forward secrecy).
//! 2. The usual mkvm messages then travel inside Noise transport messages: a
//!    u16 length, then the ciphertext (at most 64 KiB, so a video frame spans
//!    several). Pairing too.
//! 3. A paired device (a 32-byte key the server issued it, which never
//!    crosses the network again) proves its key first, with a
//!    [`device_proof`] in `Message::Device`, so its key id never travels in
//!    the clear.
//!
//! Servers before 1.0.26 hang up on [`MAGIC_NX`] at once. Plain mkvm, and
//! [`MAGIC`] (paired devices before 1.0.26), are refused.
//!
//! [`SecureReader`] and [`SecureWriter`] wrap the two halves of the socket,
//! so `read_message`/`write_message` work on them unchanged and the reading
//! and writing threads stay independent.

use std::io::{self, Read, Write};
use std::sync::Arc;

use snow::{Builder, StatelessTransportState};

/// First bytes of a paired device's connection before 1.0.26 (key id in the
/// clear, then Noise `NNpsk0`): refused now, recognised to say so.
pub const MAGIC: &[u8; 4] = b"MKVS";
/// Version of this prelude and handshake.
pub const VERSION: u8 = 1;
pub const KEY_LEN: usize = 32;
pub const KEY_ID_LEN: usize = 16;

/// First bytes of an encrypted connection.
pub const MAGIC_NX: &[u8; 4] = b"MKVN";
const NOISE_NX: &str = "Noise_NX_25519_ChaChaPoly_BLAKE2s";
/// After [`MAGIC_NX`] and the version. An older server reads the opening
/// bytes as a message header whose length is bytes 2..6 ("VN", version,
/// this): over its limit, so it hangs up at once instead of waiting.
const NX_GUARD: u8 = 0xFF;
pub const PUBLIC_KEY_LEN: usize = 32;
/// The Noise handshake hash: binds a device proof to one connection.
pub const HANDSHAKE_LEN: usize = 32;
const MAX_MESSAGE: usize = 65535;
const TAG_LEN: usize = 16;
const MAX_CHUNK: usize = MAX_MESSAGE - TAG_LEN;

/// A paired device's key, as the server issued it.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceKey {
    pub id: [u8; KEY_ID_LEN],
    pub key: [u8; KEY_LEN],
}

impl std::fmt::Debug for DeviceKey {
    // Never print the key itself.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DeviceKey({})", self.id_hex())
    }
}

impl DeviceKey {
    /// A new random key and id (from the OS).
    pub fn generate() -> io::Result<Self> {
        let mut id = [0u8; KEY_ID_LEN];
        let mut key = [0u8; KEY_LEN];
        getrandom::fill(&mut id).map_err(io::Error::other)?;
        getrandom::fill(&mut key).map_err(io::Error::other)?;
        Ok(Self { id, key })
    }

    pub fn id_hex(&self) -> String {
        hex(&self.id)
    }

    pub fn key_hex(&self) -> String {
        hex(&self.key)
    }

    pub fn from_hex(id: &str, key: &str) -> Option<Self> {
        Some(Self { id: unhex(id)?, key: unhex(key)? })
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    let s = s.trim();
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// The capture PC's long-term key pair: viewers keep the public half per
/// host and check it on every encrypted connection, so another computer on
/// the network can't pass itself off as the capture PC unnoticed.
#[derive(Clone, PartialEq, Eq)]
pub struct ServerIdentity {
    private: [u8; 32],
    public: [u8; PUBLIC_KEY_LEN],
}

impl std::fmt::Debug for ServerIdentity {
    // Never print the private key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ServerIdentity({})", hex(&self.public))
    }
}

impl ServerIdentity {
    /// A new random key pair.
    pub fn generate() -> io::Result<Self> {
        let pair = Builder::new(NOISE_NX.parse().expect("valid noise pattern")).generate_keypair().map_err(io::Error::other)?;
        let length = |_| io::Error::other("unexpected key length");
        Ok(Self { private: pair.private.try_into().map_err(length)?, public: pair.public.try_into().map_err(length)? })
    }

    pub fn from_hex(private: &str, public: &str) -> Option<Self> {
        Some(Self { private: unhex(private)?, public: unhex(public)? })
    }

    pub fn private_hex(&self) -> String {
        hex(&self.private)
    }

    pub fn public_hex(&self) -> String {
        hex(&self.public)
    }

    pub fn public(&self) -> &[u8; PUBLIC_KEY_LEN] {
        &self.public
    }
}

/// An encrypted connection, as the viewer sees it.
pub struct Opened {
    pub session: Session,
    /// The capture PC's identity (`ServerIdentity::public`).
    pub server_key: [u8; PUBLIC_KEY_LEN],
    /// For a paired device's [`device_proof`].
    pub handshake: [u8; HANDSHAKE_LEN],
}

/// What a paired device sends inside the encrypted connection to show it holds
/// its key: a keyed BLAKE2s of this connection's handshake hash, so it
/// can't be replayed on another connection.
pub fn device_proof(key: &[u8; KEY_LEN], handshake: &[u8; HANDSHAKE_LEN]) -> [u8; 32] {
    use blake2::digest::Mac;
    proof_mac(key, handshake).finalize().into_bytes().into()
}

/// Server side: whether `proof` is this key's [`device_proof`] for the
/// connection (constant time).
pub fn check_device_proof(key: &[u8; KEY_LEN], handshake: &[u8; HANDSHAKE_LEN], proof: &[u8]) -> bool {
    use blake2::digest::Mac;
    proof_mac(key, handshake).verify_slice(proof).is_ok()
}

fn proof_mac(key: &[u8; KEY_LEN], handshake: &[u8; HANDSHAKE_LEN]) -> blake2::Blake2sMac256 {
    use blake2::digest::{KeyInit, Update};
    let mut mac = <blake2::Blake2sMac256 as KeyInit>::new_from_slice(key).expect("32-byte key");
    mac.update(b"mkvm device proof 1");
    mac.update(handshake);
    mac
}

#[derive(Debug)]
pub enum SecureError {
    Io(io::Error),
    /// Not an encrypted mkvm connection, or a version we don't speak.
    Prelude(String),
    /// The handshake failed: almost always a key the other side doesn't
    /// have (revoked, or paired with another server).
    Handshake(String),
    /// The server hung up on the encrypted connection: a version from before
    /// 1.0.26.
    NotSupported,
}

impl std::fmt::Display for SecureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecureError::Io(e) => write!(f, "{e}"),
            SecureError::Prelude(e) => write!(f, "not an encrypted mkvm connection: {e}"),
            SecureError::Handshake(e) => write!(f, "encryption handshake failed: {e}"),
            SecureError::NotSupported => f.write_str("this capture server is too old for encrypted connections: update MiSTerKVMS Capture on the capture PC"),
        }
    }
}

impl std::error::Error for SecureError {}

impl From<io::Error> for SecureError {
    fn from(e: io::Error) -> Self {
        SecureError::Io(e)
    }
}

/// The agreed session: shared by the reading and the writing half.
pub type Session = Arc<StatelessTransportState>;

fn noise_err(e: snow::Error) -> SecureError {
    SecureError::Handshake(e.to_string())
}

fn write_frame(w: &mut impl Write, data: &[u8]) -> io::Result<()> {
    let mut buf = Vec::with_capacity(2 + data.len());
    buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
    buf.extend_from_slice(data);
    w.write_all(&buf)
}

fn read_frame(r: &mut impl Read, buf: &mut Vec<u8>) -> io::Result<()> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    buf.resize(u16::from_be_bytes(len) as usize, 0);
    r.read_exact(buf)
}

/// Viewer side of an encrypted connection: send the prelude and run
/// the `NX` handshake. The caller checks `server_key` against the one it
/// knows for this host.
pub fn connect_nx(sock: &mut (impl Read + Write)) -> Result<Opened, SecureError> {
    sock.write_all(&[MAGIC_NX[0], MAGIC_NX[1], MAGIC_NX[2], MAGIC_NX[3], VERSION, NX_GUARD])?;
    let mut noise = Builder::new(NOISE_NX.parse().expect("valid noise pattern")).build_initiator().map_err(noise_err)?;
    let mut buf = vec![0u8; MAX_MESSAGE];
    let n = noise.write_message(&[], &mut buf).map_err(noise_err)?;
    write_frame(sock, &buf[..n])?;
    let mut reply = Vec::new();
    read_frame(sock, &mut reply).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted => SecureError::NotSupported,
        _ => SecureError::Io(e),
    })?;
    noise.read_message(&reply, &mut buf).map_err(noise_err)?;
    let server_key: [u8; PUBLIC_KEY_LEN] = noise
        .get_remote_static()
        .and_then(|k| k.try_into().ok())
        .ok_or_else(|| SecureError::Handshake("the server sent no identity".into()))?;
    let handshake: [u8; HANDSHAKE_LEN] =
        noise.get_handshake_hash().try_into().map_err(|_| SecureError::Handshake("unexpected handshake hash".into()))?;
    Ok(Opened { session: Arc::new(noise.into_stateless_transport_mode().map_err(noise_err)?), server_key, handshake })
}

/// Server side, after the first 4 bytes were found to be [`MAGIC_NX`]: the
/// rest of the prelude.
pub fn read_prelude_nx(sock: &mut impl Read) -> Result<(), SecureError> {
    let mut rest = [0u8; 2];
    sock.read_exact(&mut rest)?;
    if rest[0] != VERSION {
        return Err(SecureError::Prelude(format!("version {}", rest[0])));
    }
    Ok(())
}

/// Server side of an encrypted connection. Returns the session and
/// the handshake hash (to check a paired device's proof against).
pub fn accept_nx(sock: &mut (impl Read + Write), identity: &ServerIdentity) -> Result<(Session, [u8; HANDSHAKE_LEN]), SecureError> {
    let mut noise = Builder::new(NOISE_NX.parse().expect("valid noise pattern"))
        .local_private_key(&identity.private)
        .map_err(noise_err)?
        .build_responder()
        .map_err(noise_err)?;
    let mut first = Vec::new();
    read_frame(sock, &mut first)?;
    let mut buf = vec![0u8; MAX_MESSAGE];
    noise.read_message(&first, &mut buf).map_err(noise_err)?;
    let n = noise.write_message(&[], &mut buf).map_err(noise_err)?;
    write_frame(sock, &buf[..n])?;
    let handshake: [u8; HANDSHAKE_LEN] =
        noise.get_handshake_hash().try_into().map_err(|_| SecureError::Handshake("unexpected handshake hash".into()))?;
    Ok((Arc::new(noise.into_stateless_transport_mode().map_err(noise_err)?), handshake))
}

/// The reading half: decrypts Noise messages into a byte stream.
pub struct SecureReader<R> {
    inner: R,
    session: Session,
    nonce: u64,
    cipher: Vec<u8>,
    plain: Vec<u8>,
    pos: usize,
}

impl<R: Read> SecureReader<R> {
    pub fn new(inner: R, session: Session) -> Self {
        Self { inner, session, nonce: 0, cipher: Vec::new(), plain: Vec::new(), pos: 0 }
    }
}

impl<R: Read> Read for SecureReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.plain.len() {
            read_frame(&mut self.inner, &mut self.cipher)?;
            self.plain.resize(MAX_MESSAGE, 0);
            let n = self
                .session
                .read_message(self.nonce, &self.cipher, &mut self.plain)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("decrypt: {e}")))?;
            self.nonce += 1;
            self.plain.truncate(n);
            self.pos = 0;
        }
        let n = out.len().min(self.plain.len() - self.pos);
        out[..n].copy_from_slice(&self.plain[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// The writing half: every write goes out at once, encrypted (as plain TCP
/// writes do; wrap it in a `BufWriter` to batch).
pub struct SecureWriter<W> {
    inner: W,
    session: Session,
    nonce: u64,
    buf: Vec<u8>,
}

impl<W: Write> SecureWriter<W> {
    pub fn new(inner: W, session: Session) -> Self {
        Self { inner, session, nonce: 0, buf: vec![0u8; MAX_MESSAGE] }
    }

    pub fn get_ref(&self) -> &W {
        &self.inner
    }
}

impl<W: Write> Write for SecureWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let chunk = &data[..data.len().min(MAX_CHUNK)];
        let n = self
            .session
            .write_message(self.nonce, chunk, &mut self.buf)
            .map_err(|e| io::Error::other(format!("encrypt: {e}")))?;
        self.nonce += 1;
        write_frame(&mut self.inner, &self.buf[..n])?;
        Ok(chunk.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Codec, Hello, Message, Video};
    use crate::wire::{read_message, write_message};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn pair() -> (TcpStream, TcpStream) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let (b, _) = l.accept().unwrap();
        (a, b)
    }

    #[test]
    fn hex_round_trip() {
        let k = DeviceKey::generate().unwrap();
        assert_eq!(DeviceKey::from_hex(&k.id_hex(), &k.key_hex()), Some(k.clone()));
        assert_eq!(k.id_hex().len(), 32);
        assert!(!format!("{k:?}").contains(&k.key_hex()));
        assert_eq!(unhex::<2>("zz00"), None);
    }

    #[test]
    fn connection_is_encrypted_and_names_the_server() {
        let identity = ServerIdentity::generate().unwrap();
        let public = *identity.public();
        let (mut client, mut server) = pair();
        let srv = thread::spawn(move || {
            let mut magic = [0u8; 4];
            server.read_exact(&mut magic).unwrap();
            assert_eq!(&magic, MAGIC_NX);
            read_prelude_nx(&mut server).unwrap();
            let (session, handshake) = accept_nx(&mut server, &identity).unwrap();
            let mut r = SecureReader::new(server.try_clone().unwrap(), session.clone());
            let mut w = SecureWriter::new(server, session);
            for _ in 0..2 {
                let (m, h) = read_message(&mut r).unwrap();
                write_message(&mut w, &m, h.timestamp).unwrap();
            }
            handshake
        });
        let opened = connect_nx(&mut client).unwrap();
        assert_eq!(opened.server_key, public);
        let mut r = SecureReader::new(client.try_clone().unwrap(), opened.session.clone());
        let mut w = SecureWriter::new(client, opened.session);
        let hello = Message::Hello(Hello {
            protocol: 1,
            stream: "MiSTer-SS1".into(),
            viewer: "Pixel".into(),
            platform: "android".into(),
            codecs: vec![Codec::Jpeg],
        });
        // Larger than one Noise message: split and reassembled.
        let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7) as u8).collect();
        let video = Message::Video(Video { codec: Codec::Jpeg, keyframe: true, width: 1280, height: 720, data });
        write_message(&mut w, &hello, 1).unwrap();
        write_message(&mut w, &video, 2).unwrap();
        assert_eq!(read_message(&mut r).unwrap().0, hello);
        assert_eq!(read_message(&mut r).unwrap().0, video);
        // Both ends agree on the handshake, so a device proof checks out.
        assert_eq!(srv.join().unwrap(), opened.handshake);
    }

    #[test]
    fn device_proofs_need_the_key_and_the_connection() {
        let key = DeviceKey::generate().unwrap();
        let other = DeviceKey::generate().unwrap();
        let handshake = [7u8; HANDSHAKE_LEN];
        let proof = device_proof(&key.key, &handshake);
        assert!(check_device_proof(&key.key, &handshake, &proof));
        assert!(!check_device_proof(&other.key, &handshake, &proof));
        assert!(!check_device_proof(&key.key, &[8u8; HANDSHAKE_LEN], &proof));
        assert!(!check_device_proof(&key.key, &handshake, &proof[..31]));
    }

    #[test]
    fn identity_hex_round_trip() {
        let id = ServerIdentity::generate().unwrap();
        assert_eq!(ServerIdentity::from_hex(&id.private_hex(), &id.public_hex()), Some(id.clone()));
        assert!(!format!("{id:?}").contains(&id.private_hex()));
    }

    #[test]
    fn an_older_server_hangs_up_at_once() {
        // What servers before key-free connections do with the opening
        // bytes: read them as a plain message, and drop the connection.
        let (mut client, server) = pair();
        let srv = thread::spawn(move || {
            let mut r = std::io::BufReader::new(server);
            assert!(matches!(read_message(&mut r), Err(crate::wire::WireError::TooLarge(_))));
        });
        assert!(matches!(connect_nx(&mut client), Err(SecureError::NotSupported)));
        srv.join().unwrap();
    }
}
